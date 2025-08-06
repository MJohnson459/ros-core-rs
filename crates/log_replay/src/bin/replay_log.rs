//! ROS Master Log Replay Tool
//!
//! This tool replays recorded ROS Master API calls from a JSONL log file against a target ROS Master
//! to validate compatibility and performance.
//!
//! ## Usage
//!
//! ```bash
//! # Basic usage - replay all requests from log file
//! cargo run --bin replay_log -- output.jsonl
//!
//! # Specify custom target URI
//! cargo run --bin replay_log -- --target-uri http://localhost:11312 output.jsonl
//!
//! # Filter by function name
//! cargo run --bin replay_log -- --function-filter getParam output.jsonl
//!
//! # Limit number of requests to replay
//! cargo run --bin replay_log -- --max-requests 100 output.jsonl
//! ```
//!
//! ## Log File Format
//!
//! The tool expects a JSONL (JSON Lines) file where each line contains a JSON object:
//!
//! ```json
//! {
//!   "request": {
//!     "timestamp": "2024-01-01T12:00:00Z",
//!     "function": "getParam",
//!     "arguments": ["/test_node", "/test_param"]
//!   },
//!   "response": {
//!     "timestamp": "2024-01-01T12:00:00Z",
//!     "status_code": 1,
//!     "message": "Parameter found",
//!     "value": "test_value",
//!     "function": "getParam"
//!   }
//! }
//! ```
//!
//! ## Testing Strategy
//!
//! 1. **Generate Log File**: Start your ROS Master with debug logging:
//!    ```bash
//!    RUST_LOG=debug cargo run --release --bin ros-core-rs |& tee output.log
//!    ```
//!
//! 2. **Convert Log to JSONL**: Convert the debug log to JSONL format:
//!    ```bash
//!    python3 scripts/convert_log_to_json.py output.log output.jsonl
//!    ```
//!
//! 3. **Replay Against Reference**: Replay the log against the reference ROS Master:
//!    ```bash
//!    rosmaster
//!    cargo run --bin replay_log -- output.jsonl
//!    ```
//!
//! 4. **Replay Against Your Implementation**: Replay the same log against your implementation:
//!    ```bash
//!    cargo run --release --bin ros-core-rs
//!    cargo run --bin replay_log -- --target-uri http://localhost:11312 output.jsonl
//!    ```
//!
//! ## Exit Codes
//!
//! - `0`: All tests passed (results match)
//! - `1`: One or more tests failed (results don't match)

use clap::Parser;
use dxr::{TryFromValue, TryToValue, Value};
use ros_core_rs::core::MasterClient;
use ros_core_rs::utils::format_value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use url::Url;

/// Represents a single log entry with request and response data
#[derive(Debug, Serialize, Deserialize)]
struct LogEntry {
    /// Optional request data (may be None for response-only entries)
    request: Option<RequestData>,
    /// Response data from the original request
    response: ResponseData,
}

/// Request data from the original API call
#[derive(Debug, Serialize, Deserialize)]
struct RequestData {
    /// Timestamp when the request was made
    timestamp: String,
    /// Function name (e.g., "getParam", "setParam", "registerPublisher")
    function: String,
    /// Function arguments as JSON values
    arguments: Vec<serde_json::Value>,
}

/// Response data from the original API call
#[derive(Debug, Serialize, Deserialize)]
struct ResponseData {
    /// Timestamp when the response was received
    timestamp: String,
    /// HTTP status code (1 = success, 0 = failure)
    status_code: Option<i32>,
    /// Response message (e.g., "Parameter found", "Parameter not found")
    message: Option<String>,
    /// Response value (can be any JSON type)
    value: Option<serde_json::Value>,
    /// Function name (should match request function)
    function: Option<String>,
}

#[derive(Parser, Debug)]
#[command(
    name = "replay_log",
    author,
    version,
    about = "Replay recorded ROS Master API calls from a JSONL log file against a target ROS Master",
    long_about = r#"
ROS Master Log Replay Tool

This tool replays recorded ROS Master API calls from a JSONL log file against a target ROS Master
to validate compatibility and performance.

EXAMPLES:
  # Basic usage - replay all requests from log file
  cargo run --bin replay_log -- output.jsonl

  # Specify custom target URI
  cargo run --bin replay_log -- --target-uri http://localhost:11312 output.jsonl

  # Filter by function name
  cargo run --bin replay_log -- --function-filter getParam output.jsonl

  # Limit number of requests to replay
  cargo run --bin replay_log -- --max-requests 100 output.jsonl

  # Suppress detailed output
  cargo run --bin replay_log -- --quiet output.jsonl

  # Continue testing even if results don't match
  cargo run --bin replay_log -- --continue output.jsonl

  # Compare message differences in addition to status and value
  cargo run --bin replay_log -- --compare-messages output.jsonl

LOG FILE FORMAT:
  The tool expects a JSONL (JSON Lines) file where each line contains a JSON object:
  {
    "request": {
      "timestamp": "2024-01-01T12:00:00Z",
      "function": "getParam",
      "arguments": ["/test_node", "/test_param"]
    },
    "response": {
      "timestamp": "2024-01-01T12:00:00Z",
      "status_code": 1,
      "message": "Parameter found",
      "value": "test_value",
      "function": "getParam"
    }
  }

TESTING STRATEGY:
  1. Generate log file: RUST_LOG=debug cargo run --release --bin ros-core-rs |& tee output.log
  2. Convert to JSONL: python3 scripts/convert_log_to_json.py output.log output.jsonl
  3. Replay against reference: rosmaster && cargo run --bin replay_log -- output.jsonl
  4. Replay against your implementation: cargo run --bin replay_log -- --target-uri http://localhost:11312 output.jsonl

EXIT CODES:
  0 - All tests passed (results match)
  1 - One or more tests failed (results don't match)
"#
)]
struct Args {
    /// JSONL log file to replay (required)
    ///
    /// The file should contain one JSON object per line with request and response data.
    /// Use the convert_log_to_json.py script to generate this file from debug logs.
    input_file: String,

    /// ROS Master URI to test against
    ///
    /// The URI of the ROS Master to replay requests against.
    /// Defaults to the standard ROS Master port.
    #[arg(short, long, default_value = "http://localhost:11311")]
    target_uri: String,

    /// Suppress detailed output for each test case
    ///
    /// By default, the tool shows detailed progress for each request.
    /// Use this flag to suppress verbose output and only show summary results.
    #[arg(short, long)]
    quiet: bool,

    /// Continue testing even if results don't match
    ///
    /// By default, the tool stops at the first mismatch.
    /// Use this flag to continue processing all requests and show all mismatches.
    #[arg(short, long)]
    continue_: bool,

    /// Maximum number of requests to replay (0 = all)
    ///
    /// Limit the number of requests to replay for quick testing.
    /// Set to 0 (default) to replay all requests in the log file.
    #[arg(short, long, default_value = "0")]
    max_requests: usize,

    /// Filter by function name (e.g., "getParam")
    ///
    /// Only replay requests for the specified function.
    /// Useful for testing specific API endpoints.
    /// Examples: "getParam", "setParam", "registerPublisher", "registerSubscriber"
    #[arg(short, long)]
    function_filter: Option<String>,

    /// Compare message differences in addition to status and value
    ///
    /// By default, the tool only compares status codes and return values.
    /// Use this flag to also compare response messages for more detailed validation.
    #[arg(short = 'g', long)]
    compare_messages: bool,
}

/// Results of replaying a log file against a target ROS Master
#[derive(Debug)]
struct ComparisonResult {
    /// Total number of requests processed
    total_requests: usize,
    /// Number of requests where results matched expected values
    matching_results: usize,
    /// Number of requests where results didn't match expected values
    mismatching_results: usize,
    /// Number of requests that failed on the target ROS Master
    target_errors: usize,
    /// Number of log entries that couldn't be parsed
    log_errors: usize,
    /// Detailed information about mismatches
    mismatches: Vec<Mismatch>,
    /// Performance timing data
    profiling: ProfilingData,
}

/// Details about a single mismatch between expected and actual results
#[derive(Debug)]
struct Mismatch {
    /// Line number in the log file where the mismatch occurred
    line_number: usize,
    /// Function name that was called
    function: String,
    /// Expected status code from the log
    expected_status: Option<i32>,
    /// Actual status code from the target ROS Master
    actual_status: Option<i32>,
    /// Expected message from the log
    expected_message: Option<String>,
    /// Actual message from the target ROS Master
    actual_message: Option<String>,
    /// Expected value from the log
    expected_value: Option<Value>,
    /// Actual value from the target ROS Master
    actual_value: Option<Value>,
    /// Error message if the target ROS Master call failed
    target_error: Option<String>,
}

/// Performance timing data for function calls
#[derive(Debug)]
struct ProfilingData {
    /// Timing data for each function (function name -> list of call durations)
    function_times: HashMap<String, Vec<Duration>>,
    /// Total execution time
    total_time: Duration,
    /// When the profiling started
    start_time: Instant,
}

impl ProfilingData {
    /// Create a new profiling data structure
    fn new() -> Self {
        Self {
            function_times: HashMap::new(),
            total_time: Duration::ZERO,
            start_time: Instant::now(),
        }
    }

    /// Record the duration of a function call for performance analysis
    fn record_call(&mut self, function: &str, duration: Duration) {
        self.function_times
            .entry(function.to_string())
            .or_insert_with(Vec::new)
            .push(duration);
    }

    /// Finalize the profiling data by calculating total execution time
    fn finish(&mut self) {
        self.total_time = self.start_time.elapsed();
    }

    /// Print a detailed performance summary showing timing for each function
    fn print_summary(&self) {
        println!("\n=== Performance Summary ===");
        println!("Total execution time: {:?}", self.total_time);
        println!();
        println!("Breakdown by endpoint:");
        println!(
            "{:<20} {:<10} {:<15} {:<15} {:<15}",
            "Function", "Calls", "Total Time", "Avg Time", "Min/Max Time"
        );
        println!("{:-<75}", "");

        let mut sorted_functions: Vec<_> = self.function_times.iter().collect();
        sorted_functions.sort_by_key(|(name, _)| *name);

        for (function, times) in sorted_functions {
            let total_time: Duration = times.iter().sum();
            let avg_time = if times.is_empty() {
                Duration::ZERO
            } else {
                total_time / times.len() as u32
            };
            let min_time = times.iter().min().unwrap_or(&Duration::ZERO);
            let max_time = times.iter().max().unwrap_or(&Duration::ZERO);

            println!(
                "{:<20} {:<10} {:<15?} {:<15?} {:<15?}",
                function,
                times.len(),
                total_time,
                avg_time,
                format!("{:?}/{:?}", min_time, max_time)
            );
        }
        println!();
    }
}

impl ComparisonResult {
    /// Create a new comparison result with empty statistics
    fn new() -> Self {
        Self {
            total_requests: 0,
            matching_results: 0,
            mismatching_results: 0,
            target_errors: 0,
            log_errors: 0,
            mismatches: Vec::new(),
            profiling: ProfilingData::new(),
        }
    }

    /// Record a successful match between expected and actual results
    fn add_match(&mut self) {
        self.matching_results += 1;
    }

    /// Record a mismatch between expected and actual results
    fn add_mismatch(&mut self, mismatch: Mismatch) {
        self.mismatching_results += 1;
        self.mismatches.push(mismatch);
    }

    /// Record an error from the target ROS Master
    fn add_target_error(&mut self) {
        self.target_errors += 1;
    }

    /// Record an error parsing the log file
    fn add_log_error(&mut self) {
        self.log_errors += 1;
    }

    fn print(&self, ignore_messages: bool) {
        println!("\n=== Replay Results ===");
        println!("Total Requests: {}", self.total_requests);
        println!();
        println!("Results Summary:");
        println!("  Matching: {}", self.matching_results);
        println!("  Mismatching: {}", self.mismatching_results);
        println!("  Target Errors: {}", self.target_errors);
        println!("  Log Errors: {}", self.log_errors);
        println!();
        if self.total_requests > 0 {
            println!(
                "Success Rate: {:.2}%",
                (self.matching_results as f64 / self.total_requests as f64) * 100.0
            );
        }

        // Print performance summary
        self.profiling.print_summary();

        if !self.mismatches.is_empty() {
            println!("\nMismatches (showing first 5):");
            for mismatch in self.mismatches.iter().take(5) {
                println!("  Line {} ({}):", mismatch.line_number, mismatch.function);
                if let Some(expected_status) = mismatch.expected_status {
                    println!("    Expected Status: {}", expected_status);
                }
                if let Some(actual_status) = mismatch.actual_status {
                    println!("    Actual Status: {}", actual_status);
                }

                if let (Some(expected_value), Some(actual_value)) =
                    (&mismatch.expected_value, &mismatch.actual_value)
                {
                    // Always show the detailed comparison for better debugging
                    println!("    Detailed comparison:");
                    let diff_info = compare_values_detailed(expected_value, actual_value);
                    for line in diff_info.lines() {
                        println!("      {}", line);
                    }

                    // Also show the full values if not in ignore_messages mode
                    if !ignore_messages {
                        println!("    Expected Value: {}", format_value(expected_value));
                        println!("    Actual Value: {}", format_value(actual_value));
                    }
                } else {
                    // Handle cases where one or both values are None
                    if let Some(expected_value) = &mismatch.expected_value {
                        println!("    Expected Value: {}", format_value(expected_value));
                    }
                    if let Some(actual_value) = &mismatch.actual_value {
                        println!("    Actual Value: {}", format_value(actual_value));
                    }
                }
                if !ignore_messages {
                    if let Some(expected_msg) = &mismatch.expected_message {
                        println!("    Expected Message: {}", expected_msg);
                    }
                    if let Some(actual_msg) = &mismatch.actual_message {
                        println!("    Actual Message: {}", actual_msg);
                    }
                }
                if let Some(target_err) = &mismatch.target_error {
                    println!("    Target Error: {}", target_err);
                }
                println!();
            }
            if self.mismatches.len() > 5 {
                println!("  ... and {} more mismatches", self.mismatches.len() - 5);
            }
        }
    }
}

/// Format a value with truncation for better debugging
///
/// This function formats values but truncates large arrays and objects to make
/// debugging easier by showing only the first few elements.
/// It recursively handles nested structures.
///
/// # Arguments
///
/// * `value` - The value to format
/// * `max_items` - Maximum number of items to show in arrays/objects
/// * `max_length` - Maximum string length before truncation
/// * `depth` - Current nesting depth (used internally for recursion)
///
/// # Returns
///
/// A formatted string with truncation
fn format_value_truncated(
    value: &Value,
    max_items: usize,
    max_length: usize,
    depth: usize,
) -> String {
    let full_str = format_value(value);

    // If we're too deep or the string is short enough, return as is
    if depth > 3 || full_str.len() <= max_length {
        if full_str.len() > max_length {
            return format!("{}...", &full_str[..max_length - 3]);
        }
        return full_str;
    }

    // For arrays, try to show first few items
    if let Ok(arr) = Vec::<Value>::try_from_value(value) {
        if arr.len() <= max_items {
            // Even for small arrays, truncate individual items if they're too long
            let mut truncated = String::new();
            truncated.push('[');

            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    truncated.push_str(", ");
                }
                let item_str = format_value_truncated(item, max_items, 100, depth + 1);
                truncated.push_str(&item_str);
            }

            truncated.push(']');
            return truncated;
        }

        let mut truncated = String::new();
        truncated.push('[');

        for (i, item) in arr.iter().take(max_items).enumerate() {
            if i > 0 {
                truncated.push_str(", ");
            }
            let item_str = format_value_truncated(item, max_items, 100, depth + 1);
            truncated.push_str(&item_str);
        }

        truncated.push_str(&format!(", ... ({} more items)]", arr.len() - max_items));
        return truncated;
    }

    // For objects/dictionaries
    if let Ok(map) = HashMap::<String, Value>::try_from_value(value) {
        if map.len() <= max_items {
            let mut truncated = String::new();
            truncated.push('{');

            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();

            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    truncated.push_str(", ");
                }
                let val_str =
                    format_value_truncated(map.get(*key).unwrap(), max_items, 50, depth + 1);
                truncated.push_str(&format!("\"{}\": {}", key, val_str));
            }

            truncated.push('}');
            return truncated;
        }

        let mut truncated = String::new();
        truncated.push('{');

        let mut keys: Vec<_> = map.keys().collect();
        keys.sort();

        for (i, key) in keys.iter().take(max_items).enumerate() {
            if i > 0 {
                truncated.push_str(", ");
            }
            let val_str = format_value_truncated(map.get(*key).unwrap(), max_items, 50, depth + 1);
            truncated.push_str(&format!("\"{}\": {}", key, val_str));
        }

        truncated.push_str(&format!(", ... ({} more keys)}}", map.len() - max_items));
        return truncated;
    }

    // For other types, just truncate the string
    if full_str.len() > max_length {
        format!("{}...", &full_str[..max_length - 3])
    } else {
        full_str
    }
}

/// Compare two values and return detailed difference information
///
/// This function provides detailed analysis of differences between expected and actual values.
/// It handles arrays, objects, and simple values with specific formatting for debugging.
/// Large values are truncated to make differences easier to spot.
///
/// # Arguments
///
/// * `expected` - The expected value from the log file
/// * `actual` - The actual value from the target ROS Master
///
/// # Returns
///
/// A formatted string describing the differences between the values
fn compare_values_detailed(expected: &Value, actual: &Value) -> String {
    if value_eq(expected, actual) {
        return "Values are identical".to_string();
    } else {
        let expected_str = format_value_truncated(expected, 5, 500, 0);
        let actual_str = format_value_truncated(actual, 5, 500, 0);

        return format!("Expected: '{}'\nActual: '{}'", expected_str, actual_str);
    }
}

fn value_eq(expected: &Value, actual: &Value) -> bool {
    if let (Ok(expected_arr), Ok(actual_arr)) = (
        Vec::<Value>::try_from_value(expected),
        Vec::<Value>::try_from_value(actual),
    ) {
        return vec_eq(&expected_arr, &actual_arr);
    }

    if let (Ok(expected_obj), Ok(actual_obj)) = (
        HashMap::<String, Value>::try_from_value(expected),
        HashMap::<String, Value>::try_from_value(actual),
    ) {
        return hash_map_eq(&expected_obj, &actual_obj);
    }

    expected == actual
}

fn vec_eq(expected: &Vec<Value>, actual: &Vec<Value>) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    for (expected, actual) in expected.iter().zip(actual.iter()) {
        if !value_eq(expected, actual) {
            return false;
        }
    }
    true
}

fn hash_map_eq(expected: &HashMap<String, Value>, actual: &HashMap<String, Value>) -> bool {
    if expected.len() != actual.len() {
        return false;
    }
    for (key, value) in expected {
        if let Some(actual_value) = actual.get(key) {
            if !value_eq(value, actual_value) {
                return false;
            }
        }
    }
    true
}

/// Convert a JSON value to the appropriate dxr::Value type
///
/// This function handles the conversion from serde_json::Value to dxr::Value,
/// which is used internally by the ROS Master client.
///
/// # Arguments
///
/// * `json_value` - The JSON value to convert
///
/// # Returns
///
/// The converted dxr::Value
fn json_to_value(json_value: &serde_json::Value) -> Value {
    match json_value {
        serde_json::Value::Null => Value::string("null".to_string()),
        serde_json::Value::Bool(b) => Value::boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::i4(i as i32)
            } else if let Some(f) = n.as_f64() {
                Value::double(f)
            } else {
                Value::string(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::string(s.clone()),
        serde_json::Value::Array(arr) => {
            let values: Vec<Value> = arr.iter().map(json_to_value).collect();
            values
                .try_to_value()
                .unwrap_or_else(|_| Value::string("[]".to_string()))
        }
        serde_json::Value::Object(obj) => {
            let mut map = HashMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), json_to_value(v));
            }
            map.try_to_value()
                .unwrap_or_else(|_| Value::string("{}".to_string()))
        }
    }
}

/// Sort an array of arrays (like getPublishedTopics result) by the first element of each sub-array
///
/// This function is used for consistent comparison of array results that may be returned
/// in different orders by different ROS Master implementations.
///
/// # Arguments
///
/// * `value` - The array value to sort
///
/// # Returns
///
/// A new Value with the array elements sorted
fn sort_array_of_arrays(value: &Value) -> Value {
    if let Ok(arr) = Vec::<Value>::try_from_value(value) {
        let mut sorted_values: Vec<Value> = arr;
        sorted_values.sort_by(|a, b| {
            let a_str = format_value(a);
            let b_str = format_value(b);
            a_str.cmp(&b_str)
        });
        sorted_values
            .try_to_value()
            .unwrap_or_else(|_| value.clone())
    } else {
        value.clone()
    }
}

/// Sort a getSystemState result which is an array of three arrays (publishers, subscribers, services)
/// Each sub-array contains topic-tuple pairs that need to be sorted
///
/// This function is used for consistent comparison of getSystemState results that may be returned
/// in different orders by different ROS Master implementations.
///
/// # Arguments
///
/// * `value` - The array value to sort
///
/// # Returns
///
/// A new Value with the array elements sorted
fn sort_internal_arrays(value: &Value) -> Value {
    // getSystemState returns: [publishers_array, subscribers_array, services_array]
    if let Ok(mut outer_arr) = Vec::<Value>::try_from_value(value) {
        // Process each of the three arrays (publishers, subscribers, services)
        for sub_array_value in &mut outer_arr {
            if let Ok(mut sub_arr) = Vec::<Vec<Value>>::try_from_value(sub_array_value) {
                // Sort the publishers/subscribers/services within each topic tuple
                for topic_tuple in &mut sub_arr {
                    if topic_tuple.len() >= 2 {
                        // The second element (index 1) should be an array of publishers/subscribers/services
                        if let Ok(entities) = Vec::<Value>::try_from_value(&topic_tuple[1]) {
                            let mut sorted_entities = entities;
                            sorted_entities.sort_by(|a, b| {
                                let a_str = format_value(a);
                                let b_str = format_value(b);
                                a_str.cmp(&b_str)
                            });
                            if let Ok(sorted_value) = sorted_entities.try_to_value() {
                                topic_tuple[1] = sorted_value;
                            }
                        }
                    }
                }

                // Sort the topic tuples by topic name (first element of each tuple)
                sub_arr.sort_by(|a, b| {
                    if a.len() > 0 && b.len() > 0 {
                        let a_str = format_value(&a[0]);
                        let b_str = format_value(&b[0]);
                        a_str.cmp(&b_str)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });

                // Update the sub-array value
                if let Ok(sorted_sub_array) = sub_arr.try_to_value() {
                    *sub_array_value = sorted_sub_array;
                }
            }
        }

        outer_arr.try_to_value().unwrap_or_else(|_| value.clone())
    } else {
        value.clone()
    }
}

/// Sort a simple array (like registerPublisher result) by string values
///
/// This function is used for consistent comparison of simple array results that may be returned
/// in different orders by different ROS Master implementations.
///
/// # Arguments
///
/// * `value` - The array value to sort
///
/// # Returns
///
/// A new Value with the array elements sorted
fn sort_simple_array(value: &Value) -> Value {
    if let Ok(arr) = Vec::<Value>::try_from_value(value) {
        let mut sorted_values: Vec<Value> = arr;
        sorted_values.sort_by(|a, b| {
            let a_str = format_value(a);
            let b_str = format_value(b);
            a_str.cmp(&b_str)
        });
        sorted_values
            .try_to_value()
            .unwrap_or_else(|_| value.clone())
    } else {
        value.clone()
    }
}

/// Sort dictionary keys for consistent comparison
///
/// This function is used for consistent comparison of dictionary results that may have
/// keys in different orders by different ROS Master implementations.
///
/// # Arguments
///
/// * `value` - The dictionary value to sort
///
/// # Returns
///
/// A new Value with the dictionary keys sorted
fn sort_dict_keys(value: &Value) -> Value {
    if let Ok(map) = HashMap::<String, Value>::try_from_value(value) {
        let mut sorted_map = HashMap::new();
        let mut keys: Vec<String> = map.keys().cloned().collect();
        keys.sort();

        for key in keys {
            if let Some(val) = map.get(&key) {
                sorted_map.insert(key, sort_dict_keys(val));
            }
        }

        sorted_map.try_to_value().unwrap_or_else(|_| value.clone())
    } else {
        value.clone()
    }
}

// --- Argument extraction helpers ---
fn get_str_arg<'a>(
    args: &'a [serde_json::Value],
    idx: usize,
    name: &str,
    func: &str,
) -> Result<&'a str, String> {
    args.get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{} requires argument {} to be a string", func, name))
}

fn get_n_str_args<'a>(
    args: &'a [serde_json::Value],
    n: usize,
    func: &str,
) -> Result<Vec<&'a str>, String> {
    if args.len() < n {
        return Err(format!("{} requires {} arguments", func, n));
    }
    (0..n)
        .map(|i| get_str_arg(args, i, &format!("{}", i + 1), func))
        .collect()
}

// --- Per-function value comparison helper ---
fn values_match(function: &str, expected: &Option<Value>, actual: &Option<Value>) -> bool {
    match function {
        "getPid" => true, // Only check status
        "subscribeParam" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                format_value(expected) == format_value(actual)
            } else {
                expected == actual
            }
        }
        _ => expected == actual,
    }
}

// --- Per-function value comparison helper ---
fn sort_values(function: &str, expected: &mut Option<Value>, actual: &mut Option<Value>) {
    match function {
        "getPublishedTopics" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                *expected = sort_array_of_arrays(expected);
                *actual = sort_array_of_arrays(actual);
            }
        }
        "getSystemState" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                *expected = sort_internal_arrays(expected);
                *actual = sort_internal_arrays(actual);
            }
        }
        "registerPublisher" | "registerSubscriber" | "getTopicTypes" | "getParamNames" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                *expected = sort_simple_array(expected);
                *actual = sort_simple_array(actual);
            }
        }
        "getParam" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                *expected = sort_dict_keys(expected);
                *actual = sort_dict_keys(actual);
            }
        }
        _ => {}
    }
}

// --- Log entry parsing/filtering helper ---
fn parse_and_filter_log_entry(line: &str, function_filter: Option<&str>) -> Option<LogEntry> {
    if line.trim().is_empty() {
        return None;
    }
    let log_entry: LogEntry = match serde_json::from_str(line) {
        Ok(entry) => entry,
        Err(_) => return None,
    };
    let request = match &log_entry.request {
        Some(req) => req,
        None => return None,
    };
    if let Some(filter) = function_filter {
        if request.function != filter {
            return None;
        }
    }
    Some(log_entry)
}

async fn call_function(
    client: &MasterClient,
    function: &str,
    args: &[serde_json::Value],
) -> Result<(i32, String, Value), String> {
    let result = match function {
        "getParam" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .get_param(strs[0], strs[1])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "setParam" => {
            if args.len() >= 3 {
                let strs = get_n_str_args(args, 2, function)?;
                let value = json_to_value(&args[2]);
                client
                    .set_param(strs[0], strs[1], &value)
                    .await
                    .map(|response| response.to_common())
                    .map_err(|e| e.to_string())
            } else {
                Err("setParam requires 3 arguments".to_string())
            }
        }
        "hasParam" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .has_param(strs[0], strs[1])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "searchParam" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .search_param(strs[0], strs[1])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "subscribeParam" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .subscribe_param(strs[0], strs[1], strs[2])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "unsubscribeParam" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .unsubscribe_param(strs[0], strs[1], strs[2])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "getParamNames" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_param_names(strs[0])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "getPid" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_pid(strs[0])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "lookupService" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .lookup_service(strs[0], strs[1])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
                .map(|response| response.to_common())
        }
        "registerPublisher" => {
            let strs = get_n_str_args(args, 4, function)?;
            client
                .register_publisher(strs[0], strs[1], strs[2], strs[3])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "unregisterPublisher" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .unregister_publisher(strs[0], strs[1], strs[2])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "registerSubscriber" => {
            let strs = get_n_str_args(args, 4, function)?;
            client
                .register_subscriber(strs[0], strs[1], strs[2], strs[3])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "unregisterSubscriber" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .unregister_subscriber(strs[0], strs[1], strs[2])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "registerService" => {
            let strs = get_n_str_args(args, 4, function)?;
            client
                .register_service(strs[0], strs[1], strs[2], strs[3])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "unregisterService" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .un_register_service(strs[0], strs[1], strs[2])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "lookupNode" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .lookup_node(strs[0], strs[1])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "getPublishedTopics" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .get_published_topics(strs[0], strs[1])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "getTopicTypes" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_topic_types(strs[0])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "getSystemState" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_system_state(strs[0])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        "getUri" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_uri(strs[0])
                .await
                .map(|response| response.to_common())
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("Unsupported function: {}", function)),
    }
    .unwrap();

    Ok((result.status, result.message, result.value))
}

/// Replay a JSONL log file against a target ROS Master
///
/// This function reads a JSONL log file line by line, parses each line as a JSON object
/// containing request and response data, and replays the requests against the target
/// ROS Master. It compares the results and provides detailed statistics.
///
/// # Arguments
///
/// * `client` - The ROS Master client to use for making requests
/// * `input_file` - Path to the JSONL log file to replay
/// * `target_uri` - URI of the target ROS Master (for display purposes)
/// * `verbose` - Whether to show detailed progress for each request
/// * `continue_on_mismatch` - Whether to continue processing after finding a mismatch
/// * `max_requests` - Maximum number of requests to process (0 = all)
/// * `function_filter` - Optional function name to filter requests
/// * `ignore_messages` - Whether to ignore message differences in comparison
///
/// # Returns
///
/// A ComparisonResult containing statistics and mismatch details
async fn replay_log(
    client: &MasterClient,
    input_file: &str,
    target_uri: &str,
    verbose: bool,
    continue_on_mismatch: bool,
    max_requests: usize,
    function_filter: Option<&str>,
    ignore_messages: bool,
) -> ComparisonResult {
    let mut result = ComparisonResult::new();

    println!("Replaying log from: {}", input_file);
    println!("Target URI: {}", target_uri);

    let file = std::fs::File::open(input_file).expect("Failed to open input file");
    let reader = std::io::BufReader::new(file);
    let lines = std::io::BufRead::lines(reader);

    for (line_num, line) in lines.enumerate() {
        if max_requests > 0 && result.total_requests >= max_requests {
            break;
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => {
                result.add_log_error();
                if verbose {
                    println!("  [{}] ✗ Failed to read line", line_num + 1);
                }
                continue;
            }
        };

        let log_entry = match parse_and_filter_log_entry(&line, function_filter) {
            Some(entry) => entry,
            None => {
                result.add_log_error();
                if verbose {
                    println!("  [{}] ✗ Invalid or filtered log entry", line_num + 1);
                }
                continue;
            }
        };

        let request = log_entry.request.as_ref().unwrap();
        result.total_requests += 1;

        // Time the function call
        let start_time = Instant::now();
        let actual_result = call_function(client, &request.function, &request.arguments).await;
        let call_duration = start_time.elapsed();

        // Record the timing
        result
            .profiling
            .record_call(&request.function, call_duration);

        let (actual_success, actual_status, actual_message, mut actual_value, target_error) =
            match &actual_result {
                Ok((status, message, value)) => (
                    true,
                    Some(*status),
                    Some(message.clone()),
                    Some(value.clone()),
                    None,
                ),
                Err(e) => (false, None, None, None, Some(e.clone())),
            };

        // Compare with expected response
        let expected_status = log_entry.response.status_code;
        let expected_message = log_entry.response.message;
        let mut expected_value = log_entry.response.value.map(|v| json_to_value(&v));

        let status_matches = expected_status == actual_status;
        let message_matches = expected_message == actual_message || ignore_messages;
        sort_values(&request.function, &mut expected_value, &mut actual_value);
        let value_matches = values_match(&request.function, &expected_value, &actual_value);

        let results_match = status_matches && message_matches && value_matches;

        if results_match {
            result.add_match();
            if verbose {
                println!(
                    "  [{}] ✓ Match: {} ({:?})",
                    line_num + 1,
                    request.function,
                    call_duration
                );
            }
        } else {
            result.add_mismatch(Mismatch {
                line_number: line_num + 1,
                function: request.function.clone(),
                expected_status,
                actual_status,
                expected_message,
                actual_message,
                expected_value,
                actual_value,
                target_error,
            });
            if verbose {
                println!(
                    "  [{}] ✗ Mismatch: {} ({:?})",
                    line_num + 1,
                    request.function,
                    call_duration
                );
            }
            if !continue_on_mismatch {
                println!("Stopping due to mismatch (use --continue to continue)");
                break;
            }
        }

        if !actual_success {
            result.add_target_error();
        }
    }

    // Finalize profiling data
    result.profiling.finish();
    result
}

#[tokio::main]
/// Main entry point for the ROS Master log replay tool
///
/// This function:
/// 1. Parses command line arguments
/// 2. Creates a ROS Master client
/// 3. Replays the log file against the target ROS Master
/// 4. Prints results and exits with appropriate code
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    // Parse the target URI
    let target_uri = Url::parse(&args.target_uri)?;
    let client = MasterClient::new(&target_uri);

    // Replay the log
    let result = replay_log(
        &client,
        &args.input_file,
        &args.target_uri,
        !args.quiet,
        args.continue_,
        args.max_requests,
        args.function_filter.as_deref(),
        !args.compare_messages,
    )
    .await;

    // Print results
    result.print(!args.compare_messages);

    // Exit with error code if there are mismatches
    if result.mismatching_results > 0 {
        std::process::exit(1);
    }

    Ok(())
}

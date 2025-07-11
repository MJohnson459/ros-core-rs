use clap::Parser;
use dxr::{TryFromValue, TryToValue, Value};
use ros_core_rs::core::MasterClient;
use ros_core_rs::utils::format_value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug, Serialize, Deserialize)]
struct LogEntry {
    request: Option<RequestData>,
    response: ResponseData,
}

#[derive(Debug, Serialize, Deserialize)]
struct RequestData {
    timestamp: String,
    function: String,
    arguments: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResponseData {
    timestamp: String,
    status_code: Option<i32>,
    message: Option<String>,
    value: Option<serde_json::Value>,
    function: Option<String>,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// JSONL log file to replay
    input_file: String,

    /// ROS Master URI to test against
    #[arg(short, long, default_value = "http://localhost:11311")]
    target_uri: String,

    /// Suppress detailed output for each test case
    #[arg(short, long)]
    quiet: bool,

    /// Continue testing even if results don't match
    #[arg(short, long)]
    continue_: bool,

    /// Maximum number of requests to replay (0 = all)
    #[arg(short, long, default_value = "0")]
    max_requests: usize,

    /// Filter by function name (e.g., "getParam")
    #[arg(short, long)]
    function_filter: Option<String>,

    /// Compare message differences in addition to status and value
    #[arg(short = 'g', long)]
    compare_messages: bool,
}

#[derive(Debug)]
struct ComparisonResult {
    total_requests: usize,
    matching_results: usize,
    mismatching_results: usize,
    target_errors: usize,
    log_errors: usize,
    mismatches: Vec<Mismatch>,
    profiling: ProfilingData,
}

#[derive(Debug)]
struct Mismatch {
    line_number: usize,
    function: String,
    expected_status: Option<i32>,
    actual_status: Option<i32>,
    expected_message: Option<String>,
    actual_message: Option<String>,
    expected_value: Option<Value>,
    actual_value: Option<Value>,
    target_error: Option<String>,
}

#[derive(Debug)]
struct ProfilingData {
    function_times: HashMap<String, Vec<Duration>>,
    total_time: Duration,
    start_time: Instant,
}

impl ProfilingData {
    fn new() -> Self {
        Self {
            function_times: HashMap::new(),
            total_time: Duration::ZERO,
            start_time: Instant::now(),
        }
    }

    fn record_call(&mut self, function: &str, duration: Duration) {
        self.function_times
            .entry(function.to_string())
            .or_insert_with(Vec::new)
            .push(duration);
    }

    fn finish(&mut self) {
        self.total_time = self.start_time.elapsed();
    }

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

    fn add_match(&mut self) {
        self.matching_results += 1;
    }

    fn add_mismatch(&mut self, mismatch: Mismatch) {
        self.mismatching_results += 1;
        self.mismatches.push(mismatch);
    }

    fn add_target_error(&mut self) {
        self.target_errors += 1;
    }

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

                // Show sorted values for functions that use sorting in comparison
                let (expected_display, actual_display) = match mismatch.function.as_str() {
                    "getPublishedTopics" => {
                        let sorted_expected = mismatch
                            .expected_value
                            .as_ref()
                            .map(|v| sort_array_of_arrays(v));
                        let sorted_actual = mismatch
                            .actual_value
                            .as_ref()
                            .map(|v| sort_array_of_arrays(v));
                        (sorted_expected, sorted_actual)
                    }
                    "registerPublisher" | "registerSubscriber" => {
                        let sorted_expected = mismatch
                            .expected_value
                            .as_ref()
                            .map(|v| sort_simple_array(v));
                        let sorted_actual =
                            mismatch.actual_value.as_ref().map(|v| sort_simple_array(v));
                        (sorted_expected, sorted_actual)
                    }
                    "getParam" => {
                        let sorted_expected =
                            mismatch.expected_value.as_ref().map(|v| sort_dict_keys(v));
                        let sorted_actual =
                            mismatch.actual_value.as_ref().map(|v| sort_dict_keys(v));
                        (sorted_expected, sorted_actual)
                    }
                    _ => {
                        // For other functions, show original values
                        (
                            mismatch.expected_value.clone(),
                            mismatch.actual_value.clone(),
                        )
                    }
                };

                if let (Some(expected_value), Some(actual_value)) =
                    (&expected_display, &actual_display)
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
                    if let Some(expected_value) = &expected_display {
                        println!("    Expected Value: {}", format_value(expected_value));
                    }
                    if let Some(actual_value) = &actual_display {
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

/// Compare two values and return detailed difference information
fn compare_values_detailed(expected: &Value, actual: &Value) -> String {
    let expected_str = format_value(expected);
    let actual_str = format_value(actual);

    if expected_str == actual_str {
        return "Values are identical".to_string();
    }

    // For arrays, try to parse and compare element by element
    if let (Ok(expected_arr), Ok(actual_arr)) = (
        Vec::<Value>::try_from_value(expected),
        Vec::<Value>::try_from_value(actual),
    ) {
        let mut diff_info = format!(
            "Array lengths: expected={}, actual={}\n",
            expected_arr.len(),
            actual_arr.len()
        );

        if expected_arr.len() != actual_arr.len() {
            diff_info.push_str(&format!(
                "Length mismatch: expected {} items, got {} items\n",
                expected_arr.len(),
                actual_arr.len()
            ));
        }

        // Convert to sets for easier comparison
        let expected_set: std::collections::HashSet<String> =
            expected_arr.iter().map(|v| format_value(v)).collect();
        let actual_set: std::collections::HashSet<String> =
            actual_arr.iter().map(|v| format_value(v)).collect();

        let only_in_expected: Vec<String> = expected_set.difference(&actual_set).cloned().collect();
        let only_in_actual: Vec<String> = actual_set.difference(&expected_set).cloned().collect();

        if !only_in_expected.is_empty() {
            diff_info.push_str(&format!(
                "\nItems only in expected ({}):\n",
                only_in_expected.len()
            ));
            for item in only_in_expected.iter() {
                // Show ALL items
                diff_info.push_str(&format!("  {}\n", item));
            }
        }

        if !only_in_actual.is_empty() {
            diff_info.push_str(&format!(
                "\nItems only in actual ({}):\n",
                only_in_actual.len()
            ));
            for item in only_in_actual.iter() {
                // Show ALL items
                diff_info.push_str(&format!("  {}\n", item));
            }
        }

        return diff_info;
    }

    // For objects, compare key by key
    if let (Ok(expected_obj), Ok(actual_obj)) = (
        HashMap::<String, Value>::try_from_value(expected),
        HashMap::<String, Value>::try_from_value(actual),
    ) {
        let mut diff_info = format!("Object comparison:\n");

        let expected_keys: std::collections::HashSet<String> =
            expected_obj.keys().cloned().collect();
        let actual_keys: std::collections::HashSet<String> = actual_obj.keys().cloned().collect();

        let only_in_expected: Vec<String> =
            expected_keys.difference(&actual_keys).cloned().collect();
        let only_in_actual: Vec<String> = actual_keys.difference(&expected_keys).cloned().collect();

        if !only_in_expected.is_empty() {
            diff_info.push_str(&format!(
                "Keys only in expected ({}): {}\n",
                only_in_expected.len(),
                only_in_expected.join(", ")
            ));
        }

        if !only_in_actual.is_empty() {
            diff_info.push_str(&format!(
                "Keys only in actual ({}): {}\n",
                only_in_actual.len(),
                only_in_actual.join(", ")
            ));
        }

        // Check for value differences in common keys
        let common_keys: Vec<String> = expected_keys.intersection(&actual_keys).cloned().collect();
        let mut value_diffs = Vec::new();

        for key in common_keys {
            if expected_obj[&key] != actual_obj[&key] {
                value_diffs.push(format!(
                    "  {}: expected '{}', got '{}'",
                    key,
                    format_value(&expected_obj[&key]),
                    format_value(&actual_obj[&key])
                ));
            }
        }

        if !value_diffs.is_empty() {
            diff_info.push_str(&format!("Value differences in common keys:\n"));
            for diff in value_diffs.iter().take(5) {
                diff_info.push_str(&format!("{}\n", diff));
            }
            if value_diffs.len() > 5 {
                diff_info.push_str(&format!(
                    "  ... and {} more differences\n",
                    value_diffs.len() - 5
                ));
            }
        }

        return diff_info;
    }

    // For simple values, just show the difference
    format!("Expected: '{}'\nActual: '{}'", expected_str, actual_str)
}

/// Convert a JSON value to the appropriate dxr::Value type
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

/// Sort a simple array (like registerPublisher result) by string values
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
        "getPublishedTopics" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                let sorted_expected = sort_array_of_arrays(expected);
                let sorted_actual = sort_array_of_arrays(actual);
                format_value(&sorted_expected) == format_value(&sorted_actual)
            } else {
                expected == actual
            }
        }
        "registerPublisher" | "registerSubscriber" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                let sorted_expected = sort_simple_array(expected);
                let sorted_actual = sort_simple_array(actual);
                sorted_expected == sorted_actual
            } else {
                expected == actual
            }
        }
        "subscribeParam" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                format_value(expected) == format_value(actual)
            } else {
                expected == actual
            }
        }
        "getParam" => {
            if let (Some(expected), Some(actual)) = (expected, actual) {
                let sorted_expected = sort_dict_keys(expected);
                let sorted_actual = sort_dict_keys(actual);
                sorted_expected == sorted_actual
            } else {
                expected == actual
            }
        }
        _ => expected == actual,
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
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, value)
                })
                .map_err(|e| e.to_string())
        }
        "setParam" => {
            if args.len() >= 3 {
                let strs = get_n_str_args(args, 2, function)?;
                let value = json_to_value(&args[2]);
                client
                    .set_param(strs[0], strs[1], &value)
                    .await
                    .map(|tuple| {
                        let (status, message, value) = tuple;
                        (status, message, Value::i4(value))
                    })
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
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::boolean(value))
                })
                .map_err(|e| e.to_string())
        }
        "searchParam" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .search_param(strs[0], strs[1])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, value)
                })
                .map_err(|e| e.to_string())
        }
        "subscribeParam" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .subscribe_param(strs[0], strs[1], strs[2])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, value)
                })
                .map_err(|e| e.to_string())
        }
        "unsubscribeParam" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .unsubscribe_param(strs[0], strs[1], strs[2])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::i4(value))
                })
                .map_err(|e| e.to_string())
        }
        "getParamNames" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_param_names(strs[0])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    let value_vec: Vec<Value> =
                        value.into_iter().map(|s| Value::string(s)).collect();
                    (
                        status,
                        message,
                        value_vec
                            .try_to_value()
                            .unwrap_or_else(|_| Value::string("[]".to_string())),
                    )
                })
                .map_err(|e| e.to_string())
        }
        "getPid" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_pid(strs[0])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::i4(value))
                })
                .map_err(|e| e.to_string())
        }
        "lookupService" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .lookup_service(strs[0], strs[1])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::string(value))
                })
                .map_err(|e| e.to_string())
        }
        "registerPublisher" => {
            let strs = get_n_str_args(args, 4, function)?;
            client
                .register_publisher(strs[0], strs[1], strs[2], strs[3])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    let value_vec: Vec<Value> =
                        value.into_iter().map(|s| Value::string(s)).collect();
                    (
                        status,
                        message,
                        value_vec
                            .try_to_value()
                            .unwrap_or_else(|_| Value::string("[]".to_string())),
                    )
                })
                .map_err(|e| e.to_string())
        }
        "unregisterPublisher" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .unregister_publisher(strs[0], strs[1], strs[2])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::i4(value))
                })
                .map_err(|e| e.to_string())
        }
        "registerSubscriber" => {
            let strs = get_n_str_args(args, 4, function)?;
            client
                .register_subscriber(strs[0], strs[1], strs[2], strs[3])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    let value_vec: Vec<Value> =
                        value.into_iter().map(|s| Value::string(s)).collect();
                    (
                        status,
                        message,
                        value_vec
                            .try_to_value()
                            .unwrap_or_else(|_| Value::string("[]".to_string())),
                    )
                })
                .map_err(|e| e.to_string())
        }
        "unregisterSubscriber" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .unregister_subscriber(strs[0], strs[1], strs[2])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::i4(value))
                })
                .map_err(|e| e.to_string())
        }
        "registerService" => {
            let strs = get_n_str_args(args, 4, function)?;
            client
                .register_service(strs[0], strs[1], strs[2], strs[3])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::i4(value))
                })
                .map_err(|e| e.to_string())
        }
        "unregisterService" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .un_register_service(strs[0], strs[1], strs[2])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::i4(value))
                })
                .map_err(|e| e.to_string())
        }
        "lookupNode" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .lookup_node(strs[0], strs[1])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::string(value))
                })
                .map_err(|e| e.to_string())
        }
        "getPublishedTopics" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .get_published_topics(strs[0], strs[1])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    let mut value_vec: Vec<Value> = value
                        .into_iter()
                        .map(|(topic_name, topic_type)| {
                            let topic_array =
                                vec![Value::string(topic_name), Value::string(topic_type)];
                            topic_array
                                .try_to_value()
                                .unwrap_or_else(|_| Value::string("[]".to_string()))
                        })
                        .collect();
                    value_vec.sort_by(|a, b| {
                        let a_str = format_value(a);
                        let b_str = format_value(b);
                        a_str.cmp(&b_str)
                    });
                    (
                        status,
                        message,
                        value_vec
                            .try_to_value()
                            .unwrap_or_else(|_| Value::string("[]".to_string())),
                    )
                })
                .map_err(|e| e.to_string())
        }
        "getTopicTypes" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_topic_types(strs[0])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    let value_vec: Vec<Value> = value
                        .into_iter()
                        .map(|(k, v)| {
                            let mut map = HashMap::new();
                            map.insert(k, Value::string(v));
                            map.try_to_value()
                                .unwrap_or_else(|_| Value::string("{}".to_string()))
                        })
                        .collect();
                    (
                        status,
                        message,
                        value_vec
                            .try_to_value()
                            .unwrap_or_else(|_| Value::string("[]".to_string())),
                    )
                })
                .map_err(|e| e.to_string())
        }
        "getSystemState" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_system_state(strs[0])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    let value_vec: Vec<Value> = value
                        .into_iter()
                        .map(|(k, v)| {
                            let v_vec: Vec<Value> =
                                v.into_iter().map(|s| Value::string(s)).collect();
                            let mut map = HashMap::new();
                            map.insert(
                                k,
                                v_vec
                                    .try_to_value()
                                    .unwrap_or_else(|_| Value::string("[]".to_string())),
                            );
                            map.try_to_value()
                                .unwrap_or_else(|_| Value::string("{}".to_string()))
                        })
                        .collect();
                    (
                        status,
                        message,
                        value_vec
                            .try_to_value()
                            .unwrap_or_else(|_| Value::string("[]".to_string())),
                    )
                })
                .map_err(|e| e.to_string())
        }
        "getUri" => {
            let strs = get_n_str_args(args, 1, function)?;
            client
                .get_uri(strs[0])
                .await
                .map(|tuple| {
                    let (status, message, value) = tuple;
                    (status, message, Value::string(value))
                })
                .map_err(|e| e.to_string())
        }
        _ => Err(format!("Unsupported function: {}", function)),
    };

    result
}

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

        let (actual_success, actual_status, actual_message, actual_value, target_error) =
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
        let expected_value = log_entry.response.value.map(|v| json_to_value(&v));

        let status_matches = expected_status == actual_status;
        let message_matches = expected_message == actual_message || ignore_messages;
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

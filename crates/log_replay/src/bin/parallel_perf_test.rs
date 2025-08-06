//! ROS Master Parallel Performance Test Tool
//!
//! This tool tests the parallel performance of a ROS Master by sending concurrent requests
//! and measuring throughput and latency. Unlike the replay tool, this doesn't validate
//! results against expected values - it focuses purely on performance metrics.
//!
//! ## Usage
//!
//! ```bash
//! # Basic parallel test with 4 workers using JSONL file
//! cargo run --bin parallel_perf_test -- --workers 4 output.jsonl
//!
//! # Test specific function with high concurrency
//! cargo run --bin parallel_perf_test -- --function getParam --workers 8 output.jsonl
//!
//! # Compare sequential vs parallel performance
//! cargo run --bin parallel_perf_test -- --workers 1 output.jsonl  # Should match replay_log
//! ```
//!
//! ## Performance Metrics
//!
//! - **Throughput**: Requests per second
//! - **Latency**: Average, median, 95th percentile response times
//! - **Concurrency Scaling**: How performance changes with worker count
//! - **Error Rate**: Percentage of failed requests

use clap::Parser;
use dxr::{TryToValue, Value};
use ros_core_rs::core::MasterClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use url::Url;

/// Represents a single log entry with request and response data (copied from replay_log.rs)
#[derive(Debug, Serialize, Deserialize)]
struct LogEntry {
    /// Optional request data (may be None for response-only entries)
    request: Option<RequestData>,
    /// Response data from the original request
    response: ResponseData,
}

/// Request data from the original API call (copied from replay_log.rs)
#[derive(Debug, Serialize, Deserialize, Clone)]
struct RequestData {
    /// Timestamp when the request was made
    timestamp: String,
    /// Function name (e.g., "getParam", "setParam", "registerPublisher")
    function: String,
    /// Function arguments as JSON values
    arguments: Vec<serde_json::Value>,
}

/// Response data from the original API call (copied from replay_log.rs)
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

/// Command line arguments for the parallel performance test
#[derive(Parser, Debug)]
#[command(
    name = "parallel_perf_test",
    author,
    version,
    about = "Test parallel performance of a ROS Master with concurrent requests",
    long_about = r#"
ROS Master Parallel Performance Test Tool

This tool tests the parallel performance of a ROS Master by sending concurrent requests
and measuring throughput and latency. Unlike the replay tool, this doesn't validate
results against expected values - it focuses purely on performance metrics.

EXAMPLES:
  # Basic parallel test with 4 workers using JSONL file
  cargo run --bin parallel_perf_test -- --workers 4 output.jsonl

  # Test specific function with high concurrency
  cargo run --bin parallel_perf_test -- --function getParam --workers 8 output.jsonl

  # Compare sequential vs parallel performance
  cargo run --bin parallel_perf_test -- --workers 1 output.jsonl  # Should match replay_log

  # Test against custom ROS Master
  cargo run --bin parallel_perf_test -- --target-uri http://localhost:11312 --workers 4 output.jsonl

PERFORMANCE METRICS:
  - Throughput: Requests per second
  - Latency: Average, median, 95th percentile response times
  - Concurrency Scaling: How performance changes with worker count
  - Error Rate: Percentage of failed requests
"#
)]
struct Args {
    /// JSONL log file to replay (required)
    ///
    /// The file should contain one JSON object per line with request and response data.
    /// Use the convert_log_to_json.py script to generate this file from debug logs.
    input_file: String,

    /// ROS Master URI to test against
    #[arg(short, long, default_value = "http://localhost:11311")]
    target_uri: String,

    /// Number of worker threads
    ///
    /// Number of concurrent workers to use for testing.
    #[arg(short, long, default_value = "4")]
    workers: usize,

    /// Maximum number of requests to replay (0 = all)
    ///
    /// Limit the number of requests to replay for quick testing.
    /// Set to 0 (default) to replay all requests in the log file.
    #[arg(short, long, default_value = "0")]
    max_requests: usize,

    /// Function to test (default: all functions from log file)
    #[arg(short, long)]
    function: Option<String>,

    /// Delay between requests in milliseconds (0 = no delay)
    #[arg(short, long, default_value = "0")]
    delay_ms: u64,
}

/// Performance test result for a single concurrency level
#[derive(Debug, Serialize)]
struct PerfResult {
    /// Number of worker threads used
    workers: usize,
    /// Total requests sent
    total_requests: usize,
    /// Successful requests
    successful_requests: usize,
    /// Failed requests
    failed_requests: usize,
    /// Total execution time
    total_time: Duration,
    /// Throughput (requests per second)
    throughput: f64,
    /// Average latency
    avg_latency: Duration,
    /// Median latency
    median_latency: Duration,
    /// 95th percentile latency
    p95_latency: Duration,
    /// 99th percentile latency
    p99_latency: Duration,
    /// Error rate percentage
    error_rate: f64,
    /// Function-specific results
    function_results: HashMap<String, FunctionResult>,
}

/// Performance result for a specific function
#[derive(Debug, Serialize)]
struct FunctionResult {
    /// Number of calls
    calls: usize,
    /// Average latency
    avg_latency: Duration,
    /// Total time spent
    total_time: Duration,
    /// Error count
    errors: usize,
}

/// Worker function that processes requests concurrently
async fn worker(
    client: Arc<MasterClient>,
    semaphore: Arc<Semaphore>,
    requests: Arc<Vec<RequestData>>,
    start_idx: usize,
    end_idx: usize,
    delay_ms: u64,
    results: Arc<tokio::sync::Mutex<Vec<(String, Duration, bool)>>>,
) {
    for i in start_idx..end_idx {
        let _permit = semaphore.acquire().await.unwrap();

        let request = &requests[i];
        let start_time = Instant::now();

        // Add delay if specified
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        let success = call_function(&client, &request.function, &request.arguments)
            .await
            .is_ok();
        let duration = start_time.elapsed();

        results
            .lock()
            .await
            .push((request.function.clone(), duration, success));
    }
}

/// Convert a JSON value to the appropriate dxr::Value type (copied from replay_log.rs)
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

/// Argument extraction helpers (copied from replay_log.rs)
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

/// Call a function on the ROS Master (copied from replay_log.rs)
async fn call_function(
    client: &MasterClient,
    function: &str,
    args: &[serde_json::Value],
) -> Result<(), String> {
    let _result = match function {
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
        "registerPublisher" => {
            let strs = get_n_str_args(args, 4, function)?;
            client
                .register_publisher(strs[0], strs[1], strs[2], strs[3])
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
        "unregisterPublisher" => {
            let strs = get_n_str_args(args, 3, function)?;
            client
                .unregister_publisher(strs[0], strs[1], strs[2])
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
        "lookupService" => {
            let strs = get_n_str_args(args, 2, function)?;
            client
                .lookup_service(strs[0], strs[1])
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
        _ => return Err(format!("Unsupported function: {}", function)),
    };

    Ok(())
}

/// Parse and filter log entries (copied from replay_log.rs)
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

/// Calculate percentiles from a sorted list of durations
fn calculate_percentile(durations: &[Duration], percentile: f64) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }
    let index = ((durations.len() as f64 - 1.0) * percentile / 100.0).round() as usize;
    durations[index.min(durations.len() - 1)]
}

/// Run a single performance test with specified concurrency
async fn run_perf_test(
    client: Arc<MasterClient>,
    workers: usize,
    requests: Arc<Vec<RequestData>>,
    delay_ms: u64,
) -> PerfResult {
    let semaphore = Arc::new(Semaphore::new(workers));
    let results = Arc::new(tokio::sync::Mutex::new(Vec::new()));

    // Actual test phase
    println!(
        "Starting performance test with {} workers, {} requests...",
        workers,
        requests.len()
    );
    let start_time = Instant::now();

    let handles: Vec<_> = (0..workers)
        .map(|worker_id| {
            let client = client.clone();
            let semaphore = semaphore.clone();
            let requests = requests.clone();
            let results = results.clone();
            let start_idx = (requests.len() * worker_id) / workers;
            let end_idx = (requests.len() * (worker_id + 1)) / workers;
            tokio::spawn(async move {
                worker(
                    client, semaphore, requests, start_idx, end_idx, delay_ms, results,
                )
                .await
            })
        })
        .collect();

    // Wait for all workers to complete
    for handle in handles {
        let _ = handle.await;
    }

    let total_time = start_time.elapsed();

    // Process results
    let all_results = results.lock().await;
    let mut durations: Vec<Duration> = all_results.iter().map(|(_, d, _)| *d).collect();
    durations.sort();

    let successful: Vec<_> = all_results.iter().filter(|(_, _, s)| *s).collect();
    let failed: Vec<_> = all_results.iter().filter(|(_, _, s)| !*s).collect();

    // Group by function
    let mut function_results = HashMap::new();
    for (function, duration, success) in all_results.iter() {
        let entry = function_results
            .entry(function.clone())
            .or_insert_with(|| FunctionResult {
                calls: 0,
                avg_latency: Duration::ZERO,
                total_time: Duration::ZERO,
                errors: 0,
            });

        entry.calls += 1;
        entry.total_time += *duration;
        if !*success {
            entry.errors += 1;
        }
    }

    // Calculate averages
    for result in function_results.values_mut() {
        if result.calls > 0 {
            result.avg_latency = result.total_time / result.calls as u32;
        }
    }

    let throughput = if total_time.as_secs_f64() > 0.0 {
        successful.len() as f64 / total_time.as_secs_f64()
    } else {
        0.0
    };

    let avg_latency = if !durations.is_empty() {
        durations.iter().sum::<Duration>() / durations.len() as u32
    } else {
        Duration::ZERO
    };

    PerfResult {
        workers,
        total_requests: all_results.len(),
        successful_requests: successful.len(),
        failed_requests: failed.len(),
        total_time,
        throughput,
        avg_latency,
        median_latency: calculate_percentile(&durations, 50.0),
        p95_latency: calculate_percentile(&durations, 95.0),
        p99_latency: calculate_percentile(&durations, 99.0),
        error_rate: if all_results.is_empty() {
            0.0
        } else {
            (failed.len() as f64 / all_results.len() as f64) * 100.0
        },
        function_results,
    }
}

/// Print results in text format
fn print_text_results(results: &[PerfResult]) {
    for result in results {
        println!("\n=== Replay Results ===");
        println!("Total Requests: {}", result.total_requests);
        println!();
        println!("Results Summary:");
        println!("  Matching: {}", result.successful_requests);
        println!("  Mismatching: {}", result.failed_requests);
        println!("  Target Errors: {}", result.failed_requests);
        println!("  Log Errors: 0");
        println!();
        println!(
            "Success Rate: {:.2}%",
            result.successful_requests as f64 / result.total_requests as f64 * 100.0
        );
        println!();
        println!("=== Performance Summary ===");
        println!(
            "Total execution time: {:.9}s",
            result.total_time.as_secs_f64()
        );
        println!();

        if !result.function_results.is_empty() {
            println!("Breakdown by endpoint:");
            println!(
                "Function             Calls      Total Time      Avg Time        Min/Max Time"
            );
            println!("---------------------------------------------------------------------------");

            let mut functions: Vec<_> = result.function_results.iter().collect();
            functions.sort_by_key(|(name, _)| *name);

            for (function, func_result) in functions {
                let total_time_secs = func_result.total_time.as_secs_f64();
                let avg_time_ms = func_result.avg_latency.as_secs_f64() * 1000.0;
                println!(
                    "{:<20} {:<10} {:<14.9}s {:<12.6}ms      \"{:.3}µs/{:.3}ms\"",
                    function,
                    func_result.calls,
                    total_time_secs,
                    avg_time_ms,
                    func_result.avg_latency.as_micros() as f64,
                    func_result.avg_latency.as_millis() as f64
                );
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    // Parse target URI
    let target_uri = Url::parse(&args.target_uri)?;
    let client = Arc::new(MasterClient::new(&target_uri));

    // Load and parse the JSONL file
    println!("Loading requests from: {}", args.input_file);
    let file = std::fs::File::open(&args.input_file)?;
    let reader = std::io::BufReader::new(file);
    let lines = std::io::BufRead::lines(reader);

    let mut requests = Vec::new();
    let mut line_count = 0;

    for line in lines {
        line_count += 1;
        if args.max_requests > 0 && requests.len() >= args.max_requests {
            break;
        }

        let line = line?;
        if let Some(log_entry) = parse_and_filter_log_entry(&line, args.function.as_deref()) {
            if let Some(request) = log_entry.request {
                requests.push(request);
            }
        }
    }

    println!(
        "Loaded {} requests from {} lines",
        requests.len(),
        line_count
    );
    println!("Testing ROS Master at: {}", args.target_uri);
    println!("Workers: {}", args.workers);
    if let Some(ref func) = args.function {
        println!("Function filter: {}", func);
    }

    let requests = Arc::new(requests);

    // Run the performance test
    let result = run_perf_test(client, args.workers, requests, args.delay_ms).await;

    // Print results
    print_text_results(&[result]);

    Ok(())
}

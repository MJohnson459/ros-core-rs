use clap::Parser;
use dxr::{TryFromValue, TryToValue, Value};
use ros_core_rs::core::MasterClient;
use ros_core_rs::utils::format_value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    #[arg(short, long)]
    input_file: String,

    /// ROS Master URI to test against
    #[arg(short, long, default_value = "http://localhost:11311")]
    target_uri: String,

    /// Show detailed results for each test case
    #[arg(short, long)]
    verbose: bool,

    /// Continue testing even if results don't match
    #[arg(short, long)]
    continue_on_mismatch: bool,

    /// Maximum number of requests to replay (0 = all)
    #[arg(short, long, default_value = "0")]
    max_requests: usize,

    /// Filter by function name (e.g., "getParam")
    #[arg(short, long)]
    function_filter: Option<String>,

    /// Ignore message differences, only compare status and value
    #[arg(short = 'g', long)]
    ignore_messages: bool,
}

#[derive(Debug)]
struct ComparisonResult {
    total_requests: usize,
    matching_results: usize,
    mismatching_results: usize,
    target_errors: usize,
    log_errors: usize,
    mismatches: Vec<Mismatch>,
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

impl ComparisonResult {
    fn new() -> Self {
        Self {
            total_requests: 0,
            matching_results: 0,
            mismatching_results: 0,
            target_errors: 0,
            log_errors: 0,
            mismatches: Vec::new(),
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

    fn print(&self) {
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
                if let Some(expected_value) = &mismatch.expected_value {
                    println!("    Expected Value: {}", format_value(expected_value));
                }
                if let Some(actual_value) = &mismatch.actual_value {
                    println!("    Actual Value: {}", format_value(actual_value));
                }
                if let Some(expected_msg) = &mismatch.expected_message {
                    println!("    Expected Message: {}", expected_msg);
                }
                if let Some(actual_msg) = &mismatch.actual_message {
                    println!("    Actual Message: {}", actual_msg);
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

/// Convert a JSON value to a string for API calls
fn json_to_string(json_value: &serde_json::Value) -> String {
    match json_value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
        }
        serde_json::Value::Object(obj) => {
            serde_json::to_string(&obj).unwrap_or_else(|_| "{}".to_string())
        }
    }
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

async fn call_function(
    client: &MasterClient,
    function: &str,
    args: &[serde_json::Value],
) -> Result<(i32, String, Value), String> {
    let result = match function {
        "getParam" => {
            if args.len() >= 2 {
                match (&args[0], &args[1]) {
                    (serde_json::Value::String(key), serde_json::Value::String(default_value)) => {
                        client
                            .get_param(&key, &default_value)
                            .await
                            .map(|tuple| {
                                let (status, message, value) = tuple;
                                (status, message, value)
                            })
                            .map_err(|e| e.to_string())
                    }
                    _ => Err("getParam requires 2 string arguments".to_string()),
                }
            } else {
                Err("getParam requires 2 arguments".to_string())
            }
        }
        "setParam" => {
            if args.len() >= 3 {
                match (&args[0], &args[1], json_to_value(&args[2])) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(key),
                        value,
                    ) => client
                        .set_param(&caller_id, &key, &value)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::i4(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("setParam requires 3 string arguments".to_string()),
                }
            } else {
                Err("setParam requires 3 arguments".to_string())
            }
        }
        "hasParam" => {
            if args.len() >= 2 {
                match (&args[0], &args[1]) {
                    (serde_json::Value::String(caller_id), serde_json::Value::String(key)) => {
                        client
                            .has_param(&caller_id, &key)
                            .await
                            .map(|tuple| {
                                let (status, message, value) = tuple;
                                (status, message, Value::boolean(value))
                            })
                            .map_err(|e| e.to_string())
                    }
                    _ => Err("hasParam requires 2 string arguments".to_string()),
                }
            } else {
                Err("hasParam requires 2 arguments".to_string())
            }
        }
        "searchParam" => {
            if args.len() >= 2 {
                match (&args[0], &args[1]) {
                    (serde_json::Value::String(caller_id), serde_json::Value::String(key)) => {
                        client
                            .search_param(&caller_id, &key)
                            .await
                            .map(|tuple| {
                                let (status, message, value) = tuple;
                                (status, message, value)
                            })
                            .map_err(|e| e.to_string())
                    }
                    _ => Err("searchParam requires 2 string arguments".to_string()),
                }
            } else {
                Err("searchParam requires 2 arguments".to_string())
            }
        }
        "subscribeParam" => {
            if args.len() >= 3 {
                match (&args[0], &args[1], &args[2]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(key),
                        serde_json::Value::String(default_value),
                    ) => client
                        .subscribe_param(&caller_id, &key, &default_value)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, value)
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("subscribeParam requires 3 string arguments".to_string()),
                }
            } else {
                Err("subscribeParam requires 3 arguments".to_string())
            }
        }
        "unsubscribeParam" => {
            if args.len() >= 3 {
                match (&args[0], &args[1], &args[2]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(key),
                        serde_json::Value::String(default_value),
                    ) => client
                        .unsubscribe_param(&caller_id, &key, &default_value)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::i4(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("unsubscribeParam requires 3 string arguments".to_string()),
                }
            } else {
                Err("unsubscribeParam requires 3 arguments".to_string())
            }
        }
        "getParamNames" => {
            if args.len() >= 1 {
                match &args[0] {
                    serde_json::Value::String(caller_id) => client
                        .get_param_names(&caller_id)
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
                        .map_err(|e| e.to_string()),
                    _ => Err("getParamNames requires 1 string argument".to_string()),
                }
            } else {
                Err("getParamNames requires 1 argument".to_string())
            }
        }
        "getPid" => {
            if args.len() >= 1 {
                match &args[0] {
                    serde_json::Value::String(caller_id) => client
                        .get_pid(&caller_id)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::i4(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("getPid requires 1 string argument".to_string()),
                }
            } else {
                Err("getPid requires 1 argument".to_string())
            }
        }
        "lookupService" => {
            if args.len() >= 2 {
                match (&args[0], &args[1]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(service_name),
                    ) => client
                        .lookup_service(&caller_id, &service_name)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::string(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("lookupService requires 2 string arguments".to_string()),
                }
            } else {
                Err("lookupService requires 2 arguments".to_string())
            }
        }
        "registerPublisher" => {
            if args.len() >= 4 {
                match (&args[0], &args[1], &args[2], &args[3]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(topic_name),
                        serde_json::Value::String(topic_type),
                        serde_json::Value::String(caller_api),
                    ) => client
                        .register_publisher(&caller_id, &topic_name, &topic_type, &caller_api)
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
                        .map_err(|e| e.to_string()),
                    _ => Err("registerPublisher requires 4 string arguments".to_string()),
                }
            } else {
                Err("registerPublisher requires 4 arguments".to_string())
            }
        }
        "unregisterPublisher" => {
            if args.len() >= 3 {
                match (&args[0], &args[1], &args[2]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(topic_name),
                        serde_json::Value::String(caller_api),
                    ) => client
                        .unregister_publisher(&caller_id, &topic_name, &caller_api)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::i4(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("unregisterPublisher requires 3 string arguments".to_string()),
                }
            } else {
                Err("unregisterPublisher requires 3 arguments".to_string())
            }
        }
        "registerSubscriber" => {
            if args.len() >= 4 {
                match (&args[0], &args[1], &args[2], &args[3]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(topic_name),
                        serde_json::Value::String(topic_type),
                        serde_json::Value::String(caller_api),
                    ) => client
                        .register_subscriber(&caller_id, &topic_name, &topic_type, &caller_api)
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
                        .map_err(|e| e.to_string()),
                    _ => Err("registerSubscriber requires 4 string arguments".to_string()),
                }
            } else {
                Err("registerSubscriber requires 4 arguments".to_string())
            }
        }
        "unregisterSubscriber" => {
            if args.len() >= 3 {
                match (&args[0], &args[1], &args[2]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(topic_name),
                        serde_json::Value::String(caller_api),
                    ) => client
                        .unregister_subscriber(&caller_id, &topic_name, &caller_api)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::i4(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("unregisterSubscriber requires 3 string arguments".to_string()),
                }
            } else {
                Err("unregisterSubscriber requires 3 arguments".to_string())
            }
        }
        "registerService" => {
            if args.len() >= 4 {
                match (&args[0], &args[1], &args[2], &args[3]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(service_name),
                        serde_json::Value::String(service_type),
                        serde_json::Value::String(caller_api),
                    ) => client
                        .register_service(&caller_id, &service_name, &service_type, &caller_api)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::i4(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("registerService requires 4 string arguments".to_string()),
                }
            } else {
                Err("registerService requires 4 arguments".to_string())
            }
        }
        "unregisterService" => {
            if args.len() >= 3 {
                match (&args[0], &args[1], &args[2]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(service_name),
                        serde_json::Value::String(caller_api),
                    ) => client
                        .un_register_service(&caller_id, &service_name, &caller_api)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::i4(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("unregisterService requires 3 string arguments".to_string()),
                }
            } else {
                Err("unregisterService requires 3 arguments".to_string())
            }
        }
        "lookupNode" => {
            if args.len() >= 2 {
                match (&args[0], &args[1]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(node_name),
                    ) => client
                        .lookup_node(&caller_id, &node_name)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::string(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("lookupNode requires 2 string arguments".to_string()),
                }
            } else {
                Err("lookupNode requires 2 arguments".to_string())
            }
        }
        "getPublishedTopics" => {
            if args.len() >= 2 {
                match (&args[0], &args[1]) {
                    (
                        serde_json::Value::String(caller_id),
                        serde_json::Value::String(node_name),
                    ) => {
                        client
                            .get_published_topics(&caller_id, &node_name)
                            .await
                            .map(|tuple| {
                                let (status, message, value) = tuple;
                                // Convert Vec<(String, String)> to array of arrays format and sort by topic name
                                let mut value_vec: Vec<Value> = value
                                    .into_iter()
                                    .map(|(topic_name, topic_type)| {
                                        let topic_array = vec![
                                            Value::string(topic_name),
                                            Value::string(topic_type),
                                        ];
                                        topic_array
                                            .try_to_value()
                                            .unwrap_or_else(|_| Value::string("[]".to_string()))
                                    })
                                    .collect();
                                // Sort by topic name for consistent comparison
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
                    _ => Err("getPublishedTopics requires 2 string arguments".to_string()),
                }
            } else {
                Err("getPublishedTopics requires 2 arguments".to_string())
            }
        }
        "getTopicTypes" => {
            if args.len() >= 1 {
                match &args[0] {
                    serde_json::Value::String(caller_id) => client
                        .get_topic_types(&caller_id)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            // Convert Vec<(String, String)> to Value
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
                        .map_err(|e| e.to_string()),
                    _ => Err("getTopicTypes requires 1 string argument".to_string()),
                }
            } else {
                Err("getTopicTypes requires 1 argument".to_string())
            }
        }
        "getSystemState" => {
            if args.len() >= 1 {
                match &args[0] {
                    serde_json::Value::String(caller_id) => client
                        .get_system_state(&caller_id)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            // Convert Vec<(String, Vec<String>)> to Value
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
                        .map_err(|e| e.to_string()),
                    _ => Err("getSystemState requires 1 string argument".to_string()),
                }
            } else {
                Err("getSystemState requires 1 argument".to_string())
            }
        }
        "getUri" => {
            if args.len() >= 1 {
                match &args[0] {
                    serde_json::Value::String(caller_id) => client
                        .get_uri(&caller_id)
                        .await
                        .map(|tuple| {
                            let (status, message, value) = tuple;
                            (status, message, Value::string(value))
                        })
                        .map_err(|e| e.to_string()),
                    _ => Err("getUri requires 1 string argument".to_string()),
                }
            } else {
                Err("getUri requires 1 argument".to_string())
            }
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
    let mut line_count = 0;

    println!("Replaying log from: {}", input_file);
    println!("Target URI: {}", target_uri);

    let file = std::fs::File::open(input_file).expect("Failed to open input file");
    let reader = std::io::BufReader::new(file);
    let lines = std::io::BufRead::lines(reader);

    for (line_num, line) in lines.enumerate() {
        line_count += 1;

        if max_requests > 0 && result.total_requests >= max_requests {
            break;
        }

        let line = line.expect("Failed to read line");
        if line.trim().is_empty() {
            continue;
        }

        let log_entry: LogEntry = match serde_json::from_str(&line) {
            Ok(entry) => entry,
            Err(e) => {
                result.add_log_error();
                if verbose {
                    println!("  [{}] ✗ JSON parse error: {}", line_num + 1, e);
                }
                continue;
            }
        };

        // Skip if no request data
        let request = match &log_entry.request {
            Some(req) => req,
            None => {
                result.add_log_error();
                if verbose {
                    println!("  [{}] ✗ No request data", line_num + 1);
                }
                continue;
            }
        };

        // Apply function filter
        if let Some(filter) = function_filter {
            if request.function != filter {
                continue;
            }
        }

        result.total_requests += 1;

        // Call the function
        let actual_result = call_function(client, &request.function, &request.arguments).await;

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
        let value_matches = if request.function == "getPid" {
            // For getPid, only check that the call succeeded (status matches)
            true
        } else if request.function == "getPublishedTopics" {
            // For getPublishedTopics, sort both arrays before comparison
            if let (Some(expected), Some(actual)) = (&expected_value, &actual_value) {
                let sorted_expected = sort_array_of_arrays(expected);
                let sorted_actual = sort_array_of_arrays(actual);
                sorted_expected == sorted_actual
            } else {
                expected_value == actual_value
            }
        } else if request.function == "registerPublisher" {
            // For registerPublisher, sort both arrays before comparison
            if let (Some(expected), Some(actual)) = (&expected_value, &actual_value) {
                let sorted_expected = sort_simple_array(expected);
                let sorted_actual = sort_simple_array(actual);
                sorted_expected == sorted_actual
            } else {
                expected_value == actual_value
            }
        } else if request.function == "registerSubscriber" {
            // For registerSubscriber, sort both arrays before comparison
            if let (Some(expected), Some(actual)) = (&expected_value, &actual_value) {
                let sorted_expected = sort_simple_array(expected);
                let sorted_actual = sort_simple_array(actual);
                sorted_expected == sorted_actual
            } else {
                expected_value == actual_value
            }
        } else if request.function == "getParam" {
            // For getParam, sort dictionary keys for consistent comparison
            if let (Some(expected), Some(actual)) = (&expected_value, &actual_value) {
                let sorted_expected = sort_dict_keys(expected);
                let sorted_actual = sort_dict_keys(actual);
                sorted_expected == sorted_actual
            } else {
                expected_value == actual_value
            }
        } else {
            expected_value == actual_value
        };

        let results_match = status_matches && message_matches && value_matches;

        if results_match {
            result.add_match();
            if verbose {
                println!("  [{}] ✓ Match: {}", line_num + 1, request.function);
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
                println!("  [{}] ✗ Mismatch: {}", line_num + 1, request.function);
            }
            if !continue_on_mismatch {
                println!("Stopping due to mismatch (use --continue-on-mismatch to continue)");
                break;
            }
        }

        if !actual_success {
            result.add_target_error();
        }
    }

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
        args.verbose,
        args.continue_on_mismatch,
        args.max_requests,
        args.function_filter.as_deref(),
        args.ignore_messages,
    )
    .await;

    // Print results
    result.print();

    // Exit with error code if there are mismatches
    if result.mismatching_results > 0 {
        std::process::exit(1);
    }

    Ok(())
}

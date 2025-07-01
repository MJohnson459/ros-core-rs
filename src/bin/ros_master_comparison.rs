use clap::{Parser, ValueEnum};
use dxr::Value;
use ros_core_rs::core::MasterClient;
use url::Url;

#[derive(Debug, Clone, ValueEnum)]
enum Endpoint {
    RegisterService,
    UnRegisterService,
    RegisterSubscriber,
    UnregisterSubscriber,
    RegisterPublisher,
    UnregisterPublisher,
    LookupNode,
    GetPublishedTopics,
    GetTopicTypes,
    GetSystemState,
    GetUri,
    LookupService,
    DeleteParam,
    SetParam,
    GetParam,
    SearchParam,
    SubscribeParam,
    UnsubscribeParam,
    HasParam,
    GetParamNames,
    GetPid,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Reference ROS Master URI
    #[arg(short, long, default_value = "http://localhost:11311")]
    reference_uri: String,

    /// Test ROS Master URI
    #[arg(short, long, default_value = "http://localhost:11312")]
    test_uri: String,

    /// Endpoint to test
    #[arg(short, long, value_enum)]
    endpoint: Endpoint,

    /// Show detailed results for each test case
    #[arg(short, long)]
    verbose: bool,

    /// Continue testing even if results don't match
    #[arg(short, long)]
    continue_on_mismatch: bool,
}

#[derive(Debug)]
struct ComparisonResult {
    endpoint: String,
    test_cases: usize,
    matching_results: usize,
    mismatching_results: usize,
    reference_errors: usize,
    test_errors: usize,
    mismatches: Vec<Mismatch>,
}

#[derive(Debug)]
struct Mismatch {
    iteration: usize,
    reference_result: String,
    test_result: String,
    reference_error: Option<String>,
    test_error: Option<String>,
}

impl ComparisonResult {
    fn new(endpoint: String, test_cases: usize) -> Self {
        Self {
            endpoint,
            test_cases,
            matching_results: 0,
            mismatching_results: 0,
            reference_errors: 0,
            test_errors: 0,
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

    fn add_reference_error(&mut self) {
        self.reference_errors += 1;
    }

    fn add_test_error(&mut self) {
        self.test_errors += 1;
    }

    fn print(&self) {
        println!("\n=== Comparison Results ===");
        println!("Endpoint: {}", self.endpoint);
        println!("Test Cases: {}", self.test_cases);
        println!();
        println!("Results Summary:");
        println!("  Matching: {}", self.matching_results);
        println!("  Mismatching: {}", self.mismatching_results);
        println!("  Reference Errors: {}", self.reference_errors);
        println!("  Test Errors: {}", self.test_errors);
        println!();
        println!(
            "Success Rate: {:.2}%",
            (self.matching_results as f64 / self.test_cases as f64) * 100.0
        );

        if !self.mismatches.is_empty() {
            println!("\nMismatches (showing first 5):");
            for (_i, mismatch) in self.mismatches.iter().take(5).enumerate() {
                println!("  Iteration {}:", mismatch.iteration);
                if let Some(ref_err) = &mismatch.reference_error {
                    println!("    Reference Error: {}", ref_err);
                } else {
                    println!("    Reference Result: {}", mismatch.reference_result);
                }
                if let Some(test_err) = &mismatch.test_error {
                    println!("    Test Error: {}", test_err);
                } else {
                    println!("    Test Result: {}", mismatch.test_result);
                }
                println!();
            }
            if self.mismatches.len() > 5 {
                println!("  ... and {} more mismatches", self.mismatches.len() - 5);
            }
        }
    }
}

struct TestData {
    caller_id: String,
    service: String,
    service_api: String,
    caller_api: String,
    topic: String,
    topic_type: String,
    node_name: String,
    param_key: String,
    param_value: Value,
    subgraph: String,
    description: String,
}

impl TestData {
    fn all_for_endpoint(endpoint: &Endpoint) -> Vec<Self> {
        match endpoint {
            Endpoint::SetParam
            | Endpoint::GetParam
            | Endpoint::DeleteParam
            | Endpoint::SearchParam
            | Endpoint::HasParam
            | Endpoint::SubscribeParam
            | Endpoint::UnsubscribeParam => vec![
                Self {
                    param_key: "/test_param1".to_string(),
                    param_value: Value::string("value1".to_string()),
                    description: "param_key=/test_param1, value=value1".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/test_param2".to_string(),
                    param_value: Value::i4(42),
                    description: "param_key=/test_param2, value=42".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/test_param3".to_string(),
                    param_value: Value::boolean(true),
                    description: "param_key=/test_param3, value=true".to_string(),
                    ..Self::default()
                },
            ],
            Endpoint::RegisterPublisher
            | Endpoint::RegisterSubscriber
            | Endpoint::UnregisterPublisher
            | Endpoint::UnregisterSubscriber
            | Endpoint::GetPublishedTopics
            | Endpoint::GetTopicTypes => vec![
                Self {
                    topic: "/test_topic1".to_string(),
                    topic_type: "std_msgs/String".to_string(),
                    description: "topic=/test_topic1, type=std_msgs/String".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/test_topic2".to_string(),
                    topic_type: "std_msgs/Int32".to_string(),
                    description: "topic=/test_topic2, type=std_msgs/Int32".to_string(),
                    ..Self::default()
                },
            ],
            Endpoint::RegisterService | Endpoint::UnRegisterService | Endpoint::LookupService => {
                vec![
                    Self {
                        service: "/test_service1".to_string(),
                        service_api: "http://localhost:12345".to_string(),
                        description: "service=/test_service1, api=http://localhost:12345"
                            .to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/test_service2".to_string(),
                        service_api: "http://localhost:12346".to_string(),
                        description: "service=/test_service2, api=http://localhost:12346"
                            .to_string(),
                        ..Self::default()
                    },
                ]
            }
            Endpoint::LookupNode | Endpoint::GetPid => vec![
                Self {
                    node_name: "/test_node1".to_string(),
                    description: "node_name=/test_node1".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/test_node2".to_string(),
                    description: "node_name=/test_node2".to_string(),
                    ..Self::default()
                },
            ],
            _ => vec![Self::default()],
        }
    }
}

impl Default for TestData {
    fn default() -> Self {
        Self {
            caller_id: "/comparison_node".to_string(),
            service: "/test_service".to_string(),
            service_api: "http://localhost:12345".to_string(),
            caller_api: "http://localhost:12346".to_string(),
            topic: "/test_topic".to_string(),
            topic_type: "std_msgs/String".to_string(),
            node_name: "/test_node".to_string(),
            param_key: "/test_param".to_string(),
            param_value: Value::string("test_value".to_string()),
            subgraph: "".to_string(),
            description: "default".to_string(),
        }
    }
}

async fn call_endpoint(
    client: &MasterClient,
    endpoint: &Endpoint,
    data: &TestData,
) -> Result<String, String> {
    let result = match endpoint {
        Endpoint::RegisterService => client
            .register_service(
                &data.caller_id,
                &data.service,
                &data.service_api,
                &data.caller_api,
            )
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::UnRegisterService => client
            .un_register_service(&data.caller_id, &data.service, &data.service_api)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::RegisterSubscriber => client
            .register_subscriber(
                &data.caller_id,
                &data.topic,
                &data.topic_type,
                &data.caller_api,
            )
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::UnregisterSubscriber => client
            .unregister_subscriber(&data.caller_id, &data.topic, &data.caller_api)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::RegisterPublisher => client
            .register_publisher(
                &data.caller_id,
                &data.topic,
                &data.topic_type,
                &data.caller_api,
            )
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::UnregisterPublisher => client
            .unregister_publisher(&data.caller_id, &data.topic, &data.caller_api)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::LookupNode => client
            .lookup_node(&data.caller_id, &data.node_name)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::GetPublishedTopics => client
            .get_published_topics(&data.caller_id, &data.subgraph)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::GetTopicTypes => client
            .get_topic_types(&data.caller_id)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::GetSystemState => client
            .get_system_state(&data.caller_id)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::GetUri => client
            .get_uri(&data.caller_id)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::LookupService => client
            .lookup_service(&data.caller_id, &data.service)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::DeleteParam => client
            .delete_param(&data.caller_id, &data.param_key)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::SetParam => client
            .set_param(&data.caller_id, &data.param_key, &data.param_value)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::GetParam => client
            .get_param(&data.caller_id, &data.param_key)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::SearchParam => client
            .search_param(&data.caller_id, &data.param_key)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::SubscribeParam => client
            .subscribe_param(&data.caller_id, &data.caller_api, &data.param_key)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::UnsubscribeParam => client
            .unsubscribe_param(&data.caller_id, &data.caller_api, &data.param_key)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::HasParam => client
            .has_param(&data.caller_id, &data.param_key)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::GetParamNames => client
            .get_param_names(&data.caller_id)
            .await
            .map(|r| format!("{:?}", r)),
        Endpoint::GetPid => client
            .get_pid(&data.caller_id)
            .await
            .map(|r| format!("{:?}", r)),
    };

    result.map_err(|e| e.to_string())
}

async fn run_comparison(
    reference_client: &MasterClient,
    test_client: &MasterClient,
    reference_uri: &str,
    test_uri: &str,
    endpoint: &Endpoint,
    test_data: &[TestData],
    verbose: bool,
    continue_on_mismatch: bool,
) -> ComparisonResult {
    let endpoint_name = format!("{:?}", endpoint);
    println!(
        "Running comparison for {} with {} test cases...",
        &endpoint_name,
        test_data.len()
    );
    println!("Reference: {}", reference_uri);
    println!("Test: {}", test_uri);

    let mut result = ComparisonResult::new(endpoint_name, test_data.len());

    for (i, data) in test_data.iter().enumerate() {
        let reference_result = call_endpoint(reference_client, endpoint, data).await;
        let test_result = call_endpoint(test_client, endpoint, data).await;

        let (reference_success, reference_value, reference_error) = match &reference_result {
            Ok(value) => (true, value.clone(), None),
            Err(e) => (false, String::new(), Some(e.clone())),
        };

        let (test_success, test_value, test_error) = match &test_result {
            Ok(value) => (true, value.clone(), None),
            Err(e) => (false, String::new(), Some(e.clone())),
        };

        let results_match = reference_success == test_success && reference_value == test_value;

        if results_match {
            result.add_match();
            if verbose {
                println!(
                    "  [{:2}/{}] ✓ Match: {}",
                    i + 1,
                    test_data.len(),
                    data.description
                );
            }
        } else {
            result.add_mismatch(Mismatch {
                iteration: i + 1,
                reference_result: reference_value,
                test_result: test_value,
                reference_error,
                test_error,
            });
            if verbose {
                println!(
                    "  [{:2}/{}] ✗ Mismatch: {}",
                    i + 1,
                    test_data.len(),
                    data.description
                );
            }
            if !continue_on_mismatch {
                println!("Stopping due to mismatch (use --continue-on-mismatch to continue)");
                break;
            }
        }
        if !reference_success {
            result.add_reference_error();
        }
        if !test_success {
            result.add_test_error();
        }
    }
    result
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    // Parse the URIs
    let reference_uri = Url::parse(&args.reference_uri)?;
    let test_uri = Url::parse(&args.test_uri)?;

    let reference_client = MasterClient::new(&reference_uri);
    let test_client = MasterClient::new(&test_uri);

    // Get test data for the endpoint
    let test_data = TestData::all_for_endpoint(&args.endpoint);

    // Run comparison
    let result = run_comparison(
        &reference_client,
        &test_client,
        &args.reference_uri,
        &args.test_uri,
        &args.endpoint,
        &test_data,
        args.verbose,
        args.continue_on_mismatch,
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

use clap::{Parser, ValueEnum};
use dxr::{TryToValue, Value};
use maplit::hashmap;
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
                // Real ROS parameters from the log
                Self {
                    param_key: "/run_id".to_string(),
                    param_value: Value::string("8a5f9a92-5696-11f0-91fc-093df97d2ac5".to_string()),
                    description: "param_key=/run_id, value=uuid".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/rosout_disable_topics_generation".to_string(),
                    param_value: Value::boolean(true),
                    description: "param_key=/rosout_disable_topics_generation, value=true".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/rosversion".to_string(),
                    param_value: Value::string("1.23.0\n".to_string()),
                    description: "param_key=/rosversion, value=1.23.0".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/rosdistro".to_string(),
                    param_value: Value::string("locusrobotics-hotdog-dev\n".to_string()),
                    description: "param_key=/rosdistro, value=locusrobotics-hotdog-dev".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/robot_lister/hb_timeout".to_string(),
                    param_value: Value::i4(30),
                    description: "param_key=/robot_lister/hb_timeout, value=30".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/dock_blocked_timeout".to_string(),
                    param_value: Value::i4(300),
                    description: "param_key=/resource_manager/dock_blocked_timeout, value=300".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/dock_choice_strategy".to_string(),
                    param_value: Value::string("random".to_string()),
                    description: "param_key=/resource_manager/dock_choice_strategy, value=random".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/universal_queue_range".to_string(),
                    param_value: Value::double(3.0),
                    description: "param_key=/resource_manager/universal_queue_range, value=3.0".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/location_footprint_factor".to_string(),
                    param_value: Value::double(1.2),
                    description: "param_key=/resource_manager/location_footprint_factor, value=1.2".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/robot_moved_threshold".to_string(),
                    param_value: Value::double(0.03),
                    description: "param_key=/resource_manager/robot_moved_threshold, value=0.03".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/robot_still_timeout".to_string(),
                    param_value: Value::double(0.5),
                    description: "param_key=/resource_manager/robot_still_timeout, value=0.5".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/use_resource_service_relays".to_string(),
                    param_value: Value::boolean(true),
                    description: "param_key=/resource_manager/use_resource_service_relays, value=true".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/resource_manager/locus_net_url".to_string(),
                    param_value: Value::string("http://localhost:5001/locusnet/botapi".to_string()),
                    description: "param_key=/resource_manager/locus_net_url, value=http://localhost:5001/locusnet/botapi".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/start_tycho_bridge/tycho_api_url".to_string(),
                    param_value: Value::string("https://staging.fleet.locusbots.io/api".to_string()),
                    description: "param_key=/start_tycho_bridge/tycho_api_url, value=https://staging.fleet.locusbots.io/api".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/start_tycho_bridge/tycho_username".to_string(),
                    param_value: Value::string("locus-services".to_string()),
                    description: "param_key=/start_tycho_bridge/tycho_username, value=locus-services".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/start_tycho_bridge/tycho_password".to_string(),
                    param_value: Value::string("DULYZNA9C7".to_string()),
                    description: "param_key=/start_tycho_bridge/tycho_password, value=DULYZNA9C7".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/central_router/reroute_interval_s".to_string(),
                    param_value: Value::i4(30),
                    description: "param_key=/central_router/reroute_interval_s, value=30".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/central_router/enable_temporal_topo_routing".to_string(),
                    param_value: Value::boolean(true),
                    description: "param_key=/central_router/enable_temporal_topo_routing, value=true".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/central_router/zone_occupancy_cost".to_string(),
                    param_value: Value::i4(200),
                    description: "param_key=/central_router/zone_occupancy_cost, value=200".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/nearby_bump_bagger/distance_threshold".to_string(),
                    param_value: Value::double(2.0),
                    description: "param_key=/nearby_bump_bagger/distance_threshold, value=2.0".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/nearby_bump_bagger/state_timeout".to_string(),
                    param_value: Value::double(15.0),
                    description: "param_key=/nearby_bump_bagger/state_timeout, value=15.0".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/diag_agg_wrangler/pub_rate".to_string(),
                    param_value: Value::double(1.0),
                    description: "param_key=/diag_agg_wrangler/pub_rate, value=1.0".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/adjutare_rosbridge_websocket_0/port".to_string(),
                    param_value: Value::i4(9090),
                    description: "param_key=/adjutare_rosbridge_websocket_0/port, value=9090".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/adjutare_rosbridge_websocket_0/use_compression".to_string(),
                    param_value: Value::boolean(true),
                    description: "param_key=/adjutare_rosbridge_websocket_0/use_compression, value=true".to_string(),
                    ..Self::default()
                },
                // Test cases for non-existent parameters
                Self {
                    param_key: "/use_sim_time".to_string(),
                    param_value: Value::boolean(false),
                    description: "param_key=/use_sim_time, value=false (non-existent)".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/tcp_keepalive".to_string(),
                    param_value: Value::boolean(false),
                    description: "param_key=/tcp_keepalive, value=false (non-existent)".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/enable_statistics".to_string(),
                    param_value: Value::boolean(false),
                    description: "param_key=/enable_statistics, value=false (non-existent)".to_string(),
                    ..Self::default()
                },
                // Test for subscribeParam with "/" key to get all parameters
                Self {
                    param_key: "/".to_string(),
                    param_value: Value::string("".to_string()),
                    description: "param_key=/, subscribeParam to get all parameters".to_string(),
                    ..Self::default()
                },
                // Test for dynamic/namespaced parameter keys
                Self {
                    param_key: "/roslaunch/uris/host_xxx".to_string(),
                    param_value: Value::string("http://localhost:11311".to_string()),
                    description: "param_key=/roslaunch/uris/host_xxx, dynamic namespaced key".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/roslaunch/uris/host_yyy".to_string(),
                    param_value: Value::string("http://localhost:11312".to_string()),
                    description: "param_key=/roslaunch/uris/host_yyy, dynamic namespaced key".to_string(),
                    ..Self::default()
                },
                // Test for setParam with dictionary value (nested structure)
                Self {
                    param_key: "/robot_config".to_string(),
                    param_value: hashmap! {
                        "name".to_string() => Value::string("robot_001".to_string()),
                        "type".to_string() => Value::string("AMR".to_string()),
                        "capabilities".to_string() => vec![
                            Value::string("navigation".to_string()),
                            Value::string("manipulation".to_string()),
                        ].try_to_value().unwrap(),
                        "settings".to_string() => hashmap! {
                            "max_speed".to_string() => Value::double(2.0),
                            "battery_threshold".to_string() => Value::double(0.2),
                            "enabled".to_string() => Value::boolean(true),
                        }.try_to_value().unwrap(),
                    }.try_to_value().unwrap(),
                    description: "param_key=/robot_config, nested dictionary value".to_string(),
                    ..Self::default()
                },
                // Test for getParam on non-existent parameter
                Self {
                    param_key: "/non_existent_parameter_12345".to_string(),
                    param_value: Value::string("".to_string()),
                    description: "param_key=/non_existent_parameter_12345, getParam on non-existent".to_string(),
                    ..Self::default()
                },
                Self {
                    param_key: "/another_missing_param".to_string(),
                    param_value: Value::string("".to_string()),
                    description: "param_key=/another_missing_param, getParam on non-existent".to_string(),
                    ..Self::default()
                },
            ],
            Endpoint::RegisterPublisher
            | Endpoint::RegisterSubscriber
            | Endpoint::UnregisterPublisher
            | Endpoint::UnregisterSubscriber
            | Endpoint::GetPublishedTopics
            | Endpoint::GetTopicTypes => vec![
                // Real topics from the log
                Self {
                    topic: "/rosout".to_string(),
                    topic_type: "rosgraph_msgs/Log".to_string(),
                    description: "topic=/rosout, type=rosgraph_msgs/Log".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/rosout_agg".to_string(),
                    topic_type: "rosgraph_msgs/Log".to_string(),
                    description: "topic=/rosout_agg, type=rosgraph_msgs/Log".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/test/robot_charge".to_string(),
                    topic_type: "locus_test_msgs/RobotCharge".to_string(),
                    description: "topic=/test/robot_charge, type=locus_test_msgs/RobotCharge".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/test/robot_error".to_string(),
                    topic_type: "locus_test_msgs/RobotError".to_string(),
                    description: "topic=/test/robot_error, type=locus_test_msgs/RobotError".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/test/robot_maintenance".to_string(),
                    topic_type: "locus_test_msgs/RobotMaintenance".to_string(),
                    description: "topic=/test/robot_maintenance, type=locus_test_msgs/RobotMaintenance".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/test/robot_status".to_string(),
                    topic_type: "locus_test_msgs/RobotStatus".to_string(),
                    description: "topic=/test/robot_status, type=locus_test_msgs/RobotStatus".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/test/robot_stop".to_string(),
                    topic_type: "locus_test_msgs/RobotStop".to_string(),
                    description: "topic=/test/robot_stop, type=locus_test_msgs/RobotStop".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/test/scenario_steps".to_string(),
                    topic_type: "locus_test_msgs/ScenarioStep".to_string(),
                    description: "topic=/test/scenario_steps, type=locus_test_msgs/ScenarioStep".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/robot_names".to_string(),
                    topic_type: "locus_msgs/StringList".to_string(),
                    description: "topic=/robot_names, type=locus_msgs/StringList".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/robot_info_array".to_string(),
                    topic_type: "locus_msgs/RobotInfoArray".to_string(),
                    description: "topic=/robot_info_array, type=locus_msgs/RobotInfoArray".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/global_states".to_string(),
                    topic_type: "locus_msgs/GlobalStateArray".to_string(),
                    description: "topic=/global_states, type=locus_msgs/GlobalStateArray".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/heartbeats".to_string(),
                    topic_type: "std_msgs/Header".to_string(),
                    description: "topic=/heartbeats, type=std_msgs/Header".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/tycho/current_site_config_local".to_string(),
                    topic_type: "*".to_string(),
                    description: "topic=/tycho/current_site_config_local, type=*".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/tycho/map".to_string(),
                    topic_type: "*".to_string(),
                    description: "topic=/tycho/map, type=*".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/central_router/snapshots".to_string(),
                    topic_type: "locus_msgs/TopoSearchQuery".to_string(),
                    description: "topic=/central_router/snapshots, type=locus_msgs/TopoSearchQuery".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/diagnostics_wrangler".to_string(),
                    topic_type: "diagnostic_msgs/DiagnosticArray".to_string(),
                    description: "topic=/diagnostics_wrangler, type=diagnostic_msgs/DiagnosticArray".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/diagnostics_toplevel_state".to_string(),
                    topic_type: "diagnostic_msgs/DiagnosticStatus".to_string(),
                    description: "topic=/diagnostics_toplevel_state, type=diagnostic_msgs/DiagnosticStatus".to_string(),
                    ..Self::default()
                },
                Self {
                    topic: "/adjutare/global_states".to_string(),
                    topic_type: "locus_msgs/GlobalStateArray".to_string(),
                    description: "topic=/adjutare/global_states, type=locus_msgs/GlobalStateArray".to_string(),
                    ..Self::default()
                },
            ],
            Endpoint::RegisterService | Endpoint::UnRegisterService | Endpoint::LookupService => {
                vec![
                    // Real services from the log
                    Self {
                        service: "/rosout/get_loggers".to_string(),
                        service_api: "rosrpc://LOCLAP858:46041".to_string(),
                        description: "service=/rosout/get_loggers, api=rosrpc://LOCLAP858:46041".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/rosout/set_logger_level".to_string(),
                        service_api: "rosrpc://LOCLAP858:46041".to_string(),
                        description: "service=/rosout/set_logger_level, api=rosrpc://LOCLAP858:46041".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/robot_lister/get_loggers".to_string(),
                        service_api: "rosrpc://LOCLAP858:48547".to_string(),
                        description: "service=/robot_lister/get_loggers, api=rosrpc://LOCLAP858:48547".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/robot_lister/set_logger_level".to_string(),
                        service_api: "rosrpc://LOCLAP858:48547".to_string(),
                        description: "service=/robot_lister/set_logger_level, api=rosrpc://LOCLAP858:48547".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/global_state_aggregator/get_loggers".to_string(),
                        service_api: "rosrpc://LOCLAP858:36461".to_string(),
                        description: "service=/global_state_aggregator/get_loggers, api=rosrpc://LOCLAP858:36461".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/global_state_aggregator/set_logger_level".to_string(),
                        service_api: "rosrpc://LOCLAP858:36461".to_string(),
                        description: "service=/global_state_aggregator/set_logger_level, api=rosrpc://LOCLAP858:36461".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/reset_global_states_connection".to_string(),
                        service_api: "rosrpc://LOCLAP858:36461".to_string(),
                        description: "service=/reset_global_states_connection, api=rosrpc://LOCLAP858:36461".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/executive_server_service_relay/get_loggers".to_string(),
                        service_api: "rosrpc://LOCLAP858:45677".to_string(),
                        description: "service=/executive_server_service_relay/get_loggers, api=rosrpc://LOCLAP858:45677".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/resource_manager/resource/request".to_string(),
                        service_api: "rosrpc://LOCLAP858:45677".to_string(),
                        description: "service=/resource_manager/resource/request, api=rosrpc://LOCLAP858:45677".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/resource_manager/resource/release".to_string(),
                        service_api: "rosrpc://LOCLAP858:45677".to_string(),
                        description: "service=/resource_manager/resource/release, api=rosrpc://LOCLAP858:45677".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/resource_manager/resource/error".to_string(),
                        service_api: "rosrpc://LOCLAP858:45677".to_string(),
                        description: "service=/resource_manager/resource/error, api=rosrpc://LOCLAP858:45677".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/resource_manager/local/resource/query".to_string(),
                        service_api: "rosrpc://LOCLAP858:45677".to_string(),
                        description: "service=/resource_manager/local/resource/query, api=rosrpc://LOCLAP858:45677".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/resource_manager/local/resource/update".to_string(),
                        service_api: "rosrpc://LOCLAP858:45677".to_string(),
                        description: "service=/resource_manager/local/resource/update, api=rosrpc://LOCLAP858:45677".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/tycho_bridge_current_site_config_relay/get_loggers".to_string(),
                        service_api: "rosrpc://LOCLAP858:41745".to_string(),
                        description: "service=/tycho_bridge_current_site_config_relay/get_loggers, api=rosrpc://LOCLAP858:41745".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/tycho_map_relay/get_loggers".to_string(),
                        service_api: "rosrpc://LOCLAP858:47591".to_string(),
                        description: "service=/tycho_map_relay/get_loggers, api=rosrpc://LOCLAP858:47591".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/central_router/get_loggers".to_string(),
                        service_api: "rosrpc://LOCLAP858:48125".to_string(),
                        description: "service=/central_router/get_loggers, api=rosrpc://LOCLAP858:48125".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/central_router/set_logger_level".to_string(),
                        service_api: "rosrpc://LOCLAP858:48125".to_string(),
                        description: "service=/central_router/set_logger_level, api=rosrpc://LOCLAP858:48125".to_string(),
                        ..Self::default()
                    },
                    Self {
                        service: "/diagnostics_agg/add_diagnostics".to_string(),
                        service_api: "rosrpc://LOCLAP858:44013".to_string(),
                        description: "service=/diagnostics_agg/add_diagnostics, api=rosrpc://LOCLAP858:44013".to_string(),
                        ..Self::default()
                    },
                ]
            }
            Endpoint::LookupNode | Endpoint::GetPid => vec![
                // Real node names from the log
                Self {
                    node_name: "/roslaunch".to_string(),
                    description: "node_name=/roslaunch".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/rosout".to_string(),
                    description: "node_name=/rosout".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/ln_rst_test_node".to_string(),
                    description: "node_name=/ln_rst_test_node".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/robot_lister".to_string(),
                    description: "node_name=/robot_lister".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/global_state_aggregator".to_string(),
                    description: "node_name=/global_state_aggregator".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/executive_server_service_relay".to_string(),
                    description: "node_name=/executive_server_service_relay".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/tycho_bridge_current_site_config_relay".to_string(),
                    description: "node_name=/tycho_bridge_current_site_config_relay".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/tycho_map_relay".to_string(),
                    description: "node_name=/tycho_map_relay".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/central_router".to_string(),
                    description: "node_name=/central_router".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/diag_agg_wrangler".to_string(),
                    description: "node_name=/diag_agg_wrangler".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/global_states_throttle".to_string(),
                    description: "node_name=/global_states_throttle".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/record".to_string(),
                    description: "node_name=/record".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/nearby_bump_bagger".to_string(),
                    description: "node_name=/nearby_bump_bagger".to_string(),
                    ..Self::default()
                },
                Self {
                    node_name: "/adjutare_rosbridge_websocket_0".to_string(),
                    description: "node_name=/adjutare_rosbridge_websocket_0".to_string(),
                    ..Self::default()
                },
                // Test non-existent nodes
                Self {
                    node_name: "/non_existent_node".to_string(),
                    description: "node_name=/non_existent_node (should fail)".to_string(),
                    ..Self::default()
                },
            ],
            Endpoint::GetSystemState => vec![
                // Test with real caller IDs from the log
                Self {
                    caller_id: "/roslaunch".to_string(),
                    description: "caller_id=/roslaunch".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/rosout".to_string(),
                    description: "caller_id=/rosout".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/ln_rst_test_node".to_string(),
                    description: "caller_id=/ln_rst_test_node".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/robot_lister".to_string(),
                    description: "caller_id=/robot_lister".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/global_state_aggregator".to_string(),
                    description: "caller_id=/global_state_aggregator".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/executive_server_service_relay".to_string(),
                    description: "caller_id=/executive_server_service_relay".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/central_router".to_string(),
                    description: "caller_id=/central_router".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/diag_agg_wrangler".to_string(),
                    description: "caller_id=/diag_agg_wrangler".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/record".to_string(),
                    description: "caller_id=/record".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/nearby_bump_bagger".to_string(),
                    description: "caller_id=/nearby_bump_bagger".to_string(),
                    ..Self::default()
                },
            ],
            Endpoint::GetUri => vec![
                // Test with real caller IDs from the log
                Self {
                    caller_id: "/roslaunch".to_string(),
                    description: "caller_id=/roslaunch".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/rosout".to_string(),
                    description: "caller_id=/rosout".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/ln_rst_test_node".to_string(),
                    description: "caller_id=/ln_rst_test_node".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/robot_lister".to_string(),
                    description: "caller_id=/robot_lister".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/global_state_aggregator".to_string(),
                    description: "caller_id=/global_state_aggregator".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/executive_server_service_relay".to_string(),
                    description: "caller_id=/executive_server_service_relay".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/central_router".to_string(),
                    description: "caller_id=/central_router".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/diag_agg_wrangler".to_string(),
                    description: "caller_id=/diag_agg_wrangler".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/record".to_string(),
                    description: "caller_id=/record".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/nearby_bump_bagger".to_string(),
                    description: "caller_id=/nearby_bump_bagger".to_string(),
                    ..Self::default()
                },
            ],
            Endpoint::GetParamNames => vec![
                // Test with real caller IDs from the log
                Self {
                    caller_id: "/roslaunch".to_string(),
                    description: "caller_id=/roslaunch".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/rosout".to_string(),
                    description: "caller_id=/rosout".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/ln_rst_test_node".to_string(),
                    description: "caller_id=/ln_rst_test_node".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/robot_lister".to_string(),
                    description: "caller_id=/robot_lister".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/global_state_aggregator".to_string(),
                    description: "caller_id=/global_state_aggregator".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/executive_server_service_relay".to_string(),
                    description: "caller_id=/executive_server_service_relay".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/central_router".to_string(),
                    description: "caller_id=/central_router".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/diag_agg_wrangler".to_string(),
                    description: "caller_id=/diag_agg_wrangler".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/record".to_string(),
                    description: "caller_id=/record".to_string(),
                    ..Self::default()
                },
                Self {
                    caller_id: "/nearby_bump_bagger".to_string(),
                    description: "caller_id=/nearby_bump_bagger".to_string(),
                    ..Self::default()
                },
            ],
        }
    }
}

impl Default for TestData {
    fn default() -> Self {
        Self {
            caller_id: "/ros_master_comparison".to_string(),
            service: "/test_service".to_string(),
            service_api: "rosrpc://localhost:12345".to_string(),
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

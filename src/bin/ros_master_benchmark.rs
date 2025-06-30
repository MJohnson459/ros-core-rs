use clap::{Parser, ValueEnum};
use dxr::Value;
use ros_core_rs::core::MasterClient;
use std::time::{Duration, Instant};
use tokio::time::sleep;
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
    /// ROS Master URI
    #[arg(short, long, default_value = "http://localhost:11311")]
    uri: String,

    /// Endpoint to benchmark
    #[arg(short, long, value_enum)]
    endpoint: Endpoint,

    /// Number of iterations to run
    #[arg(short, long, default_value_t = 1000)]
    iterations: usize,

    /// Warm-up iterations (not counted in results)
    #[arg(short, long, default_value_t = 100)]
    warmup: usize,

    /// Delay between requests in milliseconds
    #[arg(short, long, default_value_t = 0)]
    delay_ms: u64,

    /// Show detailed results for each iteration
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug)]
struct BenchmarkResult {
    endpoint: String,
    iterations: usize,
    total_time: Duration,
    avg_time: Duration,
    min_time: Duration,
    max_time: Duration,
    p50_time: Duration,
    p95_time: Duration,
    p99_time: Duration,
    success_count: usize,
    error_count: usize,
    errors: Vec<String>,
}

impl BenchmarkResult {
    fn new(endpoint: String, iterations: usize) -> Self {
        Self {
            endpoint,
            iterations,
            total_time: Duration::ZERO,
            avg_time: Duration::ZERO,
            min_time: Duration::MAX,
            max_time: Duration::ZERO,
            p50_time: Duration::ZERO,
            p95_time: Duration::ZERO,
            p99_time: Duration::ZERO,
            success_count: 0,
            error_count: 0,
            errors: Vec::new(),
        }
    }

    fn add_result(&mut self, duration: Duration, success: bool, error: Option<String>) {
        self.total_time += duration;

        if success {
            self.success_count += 1;
            self.min_time = self.min_time.min(duration);
            self.max_time = self.max_time.max(duration);
        } else {
            self.error_count += 1;
            if let Some(err) = error {
                self.errors.push(err);
            }
        }
    }

    fn finalize(&mut self, times: &[Duration]) {
        if !times.is_empty() {
            let mut sorted_times = times.to_vec();
            sorted_times.sort();

            self.avg_time = self.total_time / self.iterations as u32;
            self.p50_time = sorted_times[sorted_times.len() * 50 / 100];
            self.p95_time = sorted_times[sorted_times.len() * 95 / 100];
            self.p99_time = sorted_times[sorted_times.len() * 99 / 100];
        }
    }

    fn print(&self) {
        println!("\n=== Benchmark Results ===");
        println!("Endpoint: {}", self.endpoint);
        println!("Iterations: {}", self.iterations);
        println!(
            "Success Rate: {:.2}%",
            (self.success_count as f64 / self.iterations as f64) * 100.0
        );
        println!();
        println!("Timing Statistics:");
        println!("  Total Time: {:?}", self.total_time);
        println!("  Average: {:?}", self.avg_time);
        println!("  Min: {:?}", self.min_time);
        println!("  Max: {:?}", self.max_time);
        println!("  P50: {:?}", self.p50_time);
        println!("  P95: {:?}", self.p95_time);
        println!("  P99: {:?}", self.p99_time);
        println!();
        println!("Success/Error: {}/{}", self.success_count, self.error_count);

        if !self.errors.is_empty() {
            println!("\nErrors (showing first 5):");
            for (i, error) in self.errors.iter().take(5).enumerate() {
                println!("  {}: {}", i + 1, error);
            }
            if self.errors.len() > 5 {
                println!("  ... and {} more errors", self.errors.len() - 5);
            }
        }
    }
}

struct BenchmarkData {
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
}

impl BenchmarkData {
    fn new() -> Self {
        Self {
            caller_id: "/benchmark_node".to_string(),
            service: "/test_service".to_string(),
            service_api: "http://localhost:12345".to_string(),
            caller_api: "http://localhost:12346".to_string(),
            topic: "/test_topic".to_string(),
            topic_type: "std_msgs/String".to_string(),
            node_name: "/test_node".to_string(),
            param_key: "/test_param".to_string(),
            param_value: Value::string("test_value".to_string()),
            subgraph: "".to_string(),
        }
    }
}

async fn run_benchmark(
    client: &MasterClient,
    endpoint: &Endpoint,
    data: &BenchmarkData,
    iterations: usize,
    delay_ms: u64,
    verbose: bool,
) -> BenchmarkResult {
    let endpoint_name = format!("{:?}", endpoint);
    println!(
        "Running benchmark for {} with {} iterations...",
        &endpoint_name, &iterations
    );
    let mut result = BenchmarkResult::new(endpoint_name, iterations);
    let mut times = Vec::with_capacity(iterations);

    for i in 0..iterations {
        let start = Instant::now();
        let (success, error) = match endpoint {
            Endpoint::RegisterService => {
                match client
                    .register_service(
                        &data.caller_id,
                        &data.service,
                        &data.service_api,
                        &data.caller_api,
                    )
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::UnRegisterService => {
                match client
                    .un_register_service(&data.caller_id, &data.service, &data.service_api)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::RegisterSubscriber => {
                match client
                    .register_subscriber(
                        &data.caller_id,
                        &data.topic,
                        &data.topic_type,
                        &data.caller_api,
                    )
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::UnregisterSubscriber => {
                match client
                    .unregister_subscriber(&data.caller_id, &data.topic, &data.caller_api)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::RegisterPublisher => {
                match client
                    .register_publisher(
                        &data.caller_id,
                        &data.topic,
                        &data.topic_type,
                        &data.caller_api,
                    )
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::UnregisterPublisher => {
                match client
                    .unregister_publisher(&data.caller_id, &data.topic, &data.caller_api)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::LookupNode => {
                match client.lookup_node(&data.caller_id, &data.node_name).await {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::GetPublishedTopics => {
                match client
                    .get_published_topics(&data.caller_id, &data.subgraph)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::GetTopicTypes => match client.get_topic_types(&data.caller_id).await {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            Endpoint::GetSystemState => match client.get_system_state(&data.caller_id).await {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            Endpoint::GetUri => match client.get_uri(&data.caller_id).await {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            Endpoint::LookupService => {
                match client.lookup_service(&data.caller_id, &data.service).await {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::DeleteParam => {
                match client.delete_param(&data.caller_id, &data.param_key).await {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::SetParam => {
                match client
                    .set_param(&data.caller_id, &data.param_key, &data.param_value)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::GetParam => match client.get_param(&data.caller_id, &data.param_key).await {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            Endpoint::SearchParam => {
                match client.search_param(&data.caller_id, &data.param_key).await {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::SubscribeParam => {
                match client
                    .subscribe_param(&data.caller_id, &data.caller_api, &data.param_key)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::UnsubscribeParam => {
                match client
                    .unsubscribe_param(&data.caller_id, &data.caller_api, &data.param_key)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            Endpoint::HasParam => match client.has_param(&data.caller_id, &data.param_key).await {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            Endpoint::GetParamNames => match client.get_param_names(&data.caller_id).await {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            Endpoint::GetPid => match client.get_pid(&data.caller_id).await {
                Ok(_) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
        };

        let duration = start.elapsed();
        result.add_result(duration, success, error);

        if success {
            times.push(duration);
        }

        if verbose {
            let status = if success { "✓" } else { "✗" };
            println!("  [{:4}/{}] {} {:?}", i + 1, iterations, status, duration);
        } else if (i + 1) % 100 == 0 {
            println!("  Progress: {}/{}", i + 1, iterations);
        }

        if delay_ms > 0 {
            sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    result.finalize(&times);
    result
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Args::parse();

    // Parse the URI
    let uri = Url::parse(&args.uri)?;
    let client = MasterClient::new(&uri);

    // Create benchmark data
    let data = BenchmarkData::new();

    // Warm-up phase
    if args.warmup > 0 {
        println!("Warming up with {} iterations...", args.warmup);
        let _warmup_result =
            run_benchmark(&client, &args.endpoint, &data, args.warmup, 0, false).await;
    }

    // Actual benchmark
    let result = run_benchmark(
        &client,
        &args.endpoint,
        &data,
        args.iterations,
        args.delay_ms,
        args.verbose,
    )
    .await;

    // Print results
    result.print();

    Ok(())
}

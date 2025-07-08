#!/bin/bash

# Build the benchmark
cargo build --release

# Run the benchmark with different endpoints
echo "Running ROS Master API benchmarks..."

# Test different endpoints
endpoints=(
    "GetUri"
    "GetPid"
    "GetPublishedTopics"
    "GetTopicTypes"
    "GetSystemState"
    "GetParamNames"
    "HasParam"
    "LookupNode"
    "LookupService"
    "GetParam"
    "SearchParam"
    "SetParam"
    "DeleteParam"
    "SubscribeParam"
    "UnsubscribeParam"
    "RegisterPublisher"
    "UnregisterPublisher"
    "RegisterSubscriber"
    "UnregisterSubscriber"
    "RegisterService"
    "UnRegisterService"
)

for endpoint in "${endpoints[@]}"; do
    echo "=========================================="
    echo "Benchmarking: $endpoint"
    echo "=========================================="

    # Run with 1000 iterations, 100 warmup, no delay
    cargo run --release --bin ros_master_benchmark -- \
        --endpoint "$endpoint" \
        --iterations 1000 \
        --warmup 100 \
        --uri "http://localhost:11311"

    echo ""
done

echo "All benchmarks completed!"

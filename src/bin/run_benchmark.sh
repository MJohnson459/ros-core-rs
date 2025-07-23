#!/bin/bash

# Build the benchmark
cargo build --release

# Run the benchmark with different endpoints
echo "Running ROS Master API benchmarks..."

# Test different endpoints
endpoints=(
    "get-uri"
    "get-pid"
    "get-published-topics"
    "get-topic-types"
    "get-system-state"
    "get-param-names"
    "has-param"
    "lookup-node"
    "lookup-service"
    "get-param"
    "search-param"
    "set-param"
    "delete-param"
    "subscribe-param"
    "unsubscribe-param"
    "register-publisher"
    "unregister-publisher"
    "register-subscriber"
    "unregister-subscriber"
    "register-service"
    "un-register-service"
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

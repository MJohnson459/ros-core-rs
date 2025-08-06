# ROS-core implementation in Rust

![Rust](https://img.shields.io/badge/Rust-1.55+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)
[![Rust CI](https://github.com/PatWie/ros-core-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/PatWie/ros-core-rs/actions/workflows/ci.yml)

This Rust library provides a standalone implementation of the ROS (Robot
Operating System) core. It allows you to run a ROS master and communicate with
other ROS nodes without relying on an external ROS installation. You can use
this library to build ROS nodes entirely in Rust, including publishers and
subscribers, without needing to use any other ROS dependencies.

## Performance

We've conducted performance tests comparing our Rust implementation against a reference implementation. The tests were run with 24,401 requests using both single-threaded and multi-threaded (16 workers) configurations.

| Implementation | Threads | Total Execution Time | Success Rate | Performance Improvement |
| -------------- | ------- | -------------------- | ------------ | ----------------------- |
| Reference      | 1       | 19.21s               | 100.00%      | -                       |
| Reference      | 16      | 11.29s               | 100.00%      | 1.70x faster            |
| **Our Rust**   | **1**   | **8.24s**            | **100.00%**  | **2.33x faster**        |
| **Our Rust**   | **16**  | **3.89s**            | **100.00%**  | **4.94x faster**        |

### Key Performance Highlights

- **Single-threaded performance**: Our Rust implementation is **2.33x faster** than the reference implementation
- **Multi-threaded performance**: Our Rust implementation is **4.94x faster** than the reference implementation
- **Scalability**: Both implementations show improved performance with multiple threads, but our Rust implementation scales more efficiently
- **Reliability**: Both implementations achieve 100% success rate across all test scenarios

## Examples

### Standalone ROS core

To start the ROS core, run the following command:

```bash
# start the ros-core
RUST_LOG=debug cargo run
```

### Debugging with official ROS docker image

To showcase that this ROS core implementation can be used with official ROS
publishers and subscribers in Python, we have provided a debugging script that
launches a ROS Docker image and runs a talker and listener example. To run the
script, execute the following commands:

```bash
# change directory to debugging folder
cd debugging
# make the script executable
chmod +x run.sh
# execute the script
./run.sh
```

This script will download the official ROS Docker image and launch a container
with a ROS environment. Then, it will run a Python script that uses the ROS
talker and listener nodes to communicate with the standalone ROS core
implementation from this repository. This is intended as an example of how to use the
standalone ROS core (ros-core-rs) implementation with other ROS nodes, but it is not
necessary to use this script to use the standalone implementation on its own.

## Contributions

We welcome contributions to this project! If you find a bug or have a feature
request, please create an issue on the GitHub repository. If you want to
contribute code, feel free to submit a pull request.

## Cross-compilation to arm64

```bash
apt install libssl-dev:arm64
export AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_LIB_DIR=/usr/lib/aarch64-linux-gnu/
export AARCH64_UNKNOWN_LINUX_GNU_OPENSSL_INCLUDE_DIR=/usr/include/aarch64-linux-gnu/
cargo build --target aarch64-unknown-linux-gnu --release
```

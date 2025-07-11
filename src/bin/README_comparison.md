# ROS Master Comparison Tool

This tool compares the behavior of two ROS Master implementations by calling the same endpoints on both and verifying that the results match.

## Usage

### Command Line Interface

```bash
# Basic usage - compare against default ports
cargo run --bin ros_master_comparison -- --endpoint GetParam

# Specify custom URIs
cargo run --bin ros_master_comparison -- \
    --reference-uri http://localhost:11311 \
    --test-uri http://localhost:11312 \
    --endpoint RegisterService \
    --verbose

# Run with custom parameters
cargo run --bin ros_master_comparison -- \
    --reference-uri http://reference-master:11311 \
    --test-uri http://test-master:11312 \
    --endpoint GetSystemState \
    --continue-on-mismatch
```

### Shell Script

```bash
# Run all endpoint tests
./src/bin/run_comparison.sh

# Test a specific endpoint
./src/bin/run_comparison.sh GetParam

# Use environment variables for configuration
REFERENCE_URI=http://localhost:11311 \
TEST_URI=http://localhost:11312 \
VERBOSE=true \
./src/bin/run_comparison.sh RegisterService
```

## Command Line Options

| Option                   | Short | Description                  | Default                  |
| ------------------------ | ----- | ---------------------------- | ------------------------ |
| `--reference-uri`        | `-r`  | Reference ROS Master URI     | `http://localhost:11311` |
| `--test-uri`             | `-t`  | Test ROS Master URI          | `http://localhost:11312` |
| `--endpoint`             | `-e`  | Endpoint to test (required)  | -                        |
| `--verbose`              | `-v`  | Show detailed results        | `false`                  |
| `--continue-on-mismatch` | `-c`  | Continue testing on mismatch | `false`                  |

## Environment Variables (for shell script)

| Variable               | Description                  | Default                  |
| ---------------------- | ---------------------------- | ------------------------ |
| `REFERENCE_URI`        | Reference ROS Master URI     | `http://localhost:11312` |
| `TEST_URI`             | Test ROS Master URI          | `http://localhost:11311` |
| `VERBOSE`              | Enable verbose output        | `false`                  |
| `CONTINUE_ON_MISMATCH` | Continue testing on mismatch | `false`                  |

## Available Endpoints

- **Service Management**: `RegisterService`, `UnRegisterService`, `LookupService`
- **Topic Management**: `RegisterSubscriber`, `UnregisterSubscriber`, `RegisterPublisher`, `UnregisterPublisher`
- **Node Management**: `LookupNode`, `GetPid`
- **System Information**: `GetPublishedTopics`, `GetTopicTypes`, `GetSystemState`, `GetUri`
- **Parameter Management**: `DeleteParam`, `SetParam`, `GetParam`, `SearchParam`, `SubscribeParam`, `UnsubscribeParam`, `HasParam`, `GetParamNames`

## Exit Codes

- `0`: All tests passed (results match)
- `1`: One or more tests failed (results don't match)

## Testing Strategy

### 1. Setup Two ROS Masters

```bash
# Start reference ROS Master
roscore -p 11311

# Start your test ROS Master
./your_ros_master --port 11312
```

### 2. Run Comparison Tests

```bash
# Quick test of a single endpoint
./src/bin/run_comparison.sh GetParam

# Comprehensive test of all endpoints
./src/bin/run_comparison.sh

# Verbose testing for debugging
VERBOSE=true ./src/bin/run_comparison.sh RegisterService
```

## Troubleshooting

### Common Issues

1. **Connection Refused**: Ensure both ROS Masters are running and accessible
2. **Timeout Errors**: Check network connectivity and ensure both ROS Masters are accessible
3. **Permission Denied**: Ensure the script is executable (`chmod +x src/bin/run_comparison.sh`)

### Debugging Mismatches

1. Use `--verbose` flag to see detailed results for each test case
2. Use `--continue-on-mismatch` to see all mismatches instead of stopping at the first one
3. Check the specific error messages and result values in the mismatch details

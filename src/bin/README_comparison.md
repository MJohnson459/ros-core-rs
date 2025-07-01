# ROS Master Comparison Tool

This tool compares the behavior of two ROS Master implementations by calling the same endpoints on both and verifying that the results match.

## Overview

The comparison tool is designed to validate that your ROS Master implementation produces the same results as a reference implementation (typically the official ROS Master). It can be used for:

- Regression testing during development
- Validating compatibility with the ROS Master API
- Debugging differences between implementations
- Continuous integration testing

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
    --iterations 5 \
    --verbose

# Run with custom parameters
cargo run --bin ros_master_comparison -- \
    --reference-uri http://reference-master:11311 \
    --test-uri http://test-master:11312 \
    --endpoint GetSystemState \
    --iterations 20 \
    --delay-ms 200 \
    --continue-on-mismatch
```

### Shell Script

For easier usage, you can use the provided shell script:

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
| `--iterations`           | `-i`  | Number of iterations         | `10`                     |
| `--delay-ms`             | `-d`  | Delay between requests (ms)  | `100`                    |
| `--verbose`              | `-v`  | Show detailed results        | `false`                  |
| `--continue-on-mismatch` | `-c`  | Continue testing on mismatch | `false`                  |

## Environment Variables (for shell script)

| Variable               | Description                   | Default                  |
| ---------------------- | ----------------------------- | ------------------------ |
| `REFERENCE_URI`        | Reference ROS Master URI      | `http://localhost:11311` |
| `TEST_URI`             | Test ROS Master URI           | `http://localhost:11312` |
| `ITERATIONS`           | Number of iterations per test | `10`                     |
| `DELAY_MS`             | Delay between requests in ms  | `100`                    |
| `VERBOSE`              | Enable verbose output         | `false`                  |
| `CONTINUE_ON_MISMATCH` | Continue testing on mismatch  | `false`                  |

## Available Endpoints

The tool supports all ROS Master API endpoints:

- **Service Management**: `RegisterService`, `UnRegisterService`, `LookupService`
- **Topic Management**: `RegisterSubscriber`, `UnregisterSubscriber`, `RegisterPublisher`, `UnregisterPublisher`
- **Node Management**: `LookupNode`, `GetPid`
- **System Information**: `GetPublishedTopics`, `GetTopicTypes`, `GetSystemState`, `GetUri`
- **Parameter Management**: `DeleteParam`, `SetParam`, `GetParam`, `SearchParam`, `SubscribeParam`, `UnsubscribeParam`, `HasParam`, `GetParamNames`

## Output

The tool provides detailed output including:

- **Summary Statistics**: Matching/mismatching results, error counts
- **Success Rate**: Percentage of successful comparisons
- **Mismatch Details**: Specific differences when results don't match
- **Error Information**: Details about failures on either implementation

### Example Output

```
=== Comparison Results ===
Endpoint: GetParam
Iterations: 10

Results Summary:
  Matching: 8
  Mismatching: 2
  Reference Errors: 0
  Test Errors: 0

Success Rate: 80.00%

Mismatches (showing first 5):
  Iteration 3:
    Reference Result: "test_value"
    Test Result: "different_value"

  Iteration 7:
    Reference Result: "test_value"
    Test Result: "different_value"
```

## Exit Codes

- `0`: All tests passed (results match)
- `1`: One or more tests failed (results don't match)

## Testing Strategy

### 1. Setup Two ROS Masters

Start your reference ROS Master (e.g., official ROS Master):

```bash
roscore -p 11311
```

Start your test ROS Master on a different port:

```bash
# Your implementation
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

### 3. Analyze Results

- **All tests pass**: Your implementation is compatible
- **Some tests fail**: Review the mismatch details to understand differences
- **All tests fail**: Check if your ROS Master is running and accessible

## Troubleshooting

### Common Issues

1. **Connection Refused**: Ensure both ROS Masters are running and accessible
2. **Timeout Errors**: Increase delay between requests or check network connectivity
3. **Permission Denied**: Ensure the script is executable (`chmod +x src/bin/run_comparison.sh`)

### Debugging Mismatches

1. Use `--verbose` flag to see detailed results for each iteration
2. Use `--continue-on-mismatch` to see all mismatches instead of stopping at the first one
3. Check the specific error messages and result values in the mismatch details

### Performance Considerations

- Use fewer iterations for quick testing (`--iterations 5`)
- Increase delay between requests if servers are overloaded (`--delay-ms 500`)
- Use `--continue-on-mismatch` for comprehensive testing

## Integration with CI/CD

The tool can be integrated into continuous integration pipelines:

```yaml
# Example GitHub Actions step
- name: Test ROS Master Compatibility
  run: |
    # Start reference ROS Master
    roscore -p 11311 &
    sleep 5

    # Start test ROS Master
    ./your_ros_master --port 11312 &
    sleep 5

    # Run comparison tests
    ./src/bin/run_comparison.sh
```

The tool will exit with code 1 if any mismatches are found, making it suitable for automated testing.

# ROS Master Log Replay Tool

This tool replays recorded ROS Master API calls from a JSONL log file against a target ROS Master to validate compatibility and performance.

## Usage

### Command Line Interface

```bash
# Basic usage - replay all requests from log file
cargo run --bin replay_log -- output.jsonl

# Specify custom target URI
cargo run --bin replay_log -- \
    --target-uri http://localhost:11312 \
    output.jsonl

# Filter by function name
cargo run --bin replay_log -- \
    --function-filter getParam \
    output.jsonl

# Limit number of requests to replay
cargo run --bin replay_log -- \
    --max-requests 100 \
    output.jsonl

# Suppress detailed output
cargo run --bin replay_log -- \
    --quiet \
    output.jsonl

# Continue testing even if results don't match
cargo run --bin replay_log -- \
    --continue \
    output.jsonl

# Compare message differences in addition to status and value
cargo run --bin replay_log -- \
    --compare-messages \
    output.jsonl
```

## Command Line Options

| Option               | Short | Description                    | Default                      |
| -------------------- | ----- | ------------------------------ | ---------------------------- |
| `input_file`         | -     | JSONL log file to replay       | (required)                   |
| `--target-uri`       | `-t`  | Target ROS Master URI          | `http://localhost:11311`     |
| `--quiet`            | `-q`  | Suppress detailed output       | `false` (verbose by default) |
| `--continue`         | `-c`  | Continue on mismatch           | `false`                      |
| `--max-requests`     | `-m`  | Max requests to replay (0=all) | `0`                          |
| `--function-filter`  | `-f`  | Filter by function name        | `None`                       |
| `--compare-messages` | `-g`  | Compare message differences    | `false`                      |

## Log File Format

The tool expects a JSONL (JSON Lines) file where each line contains a JSON object:

```json
{
  "request": {
    "timestamp": "2024-01-01T12:00:00Z",
    "function": "getParam",
    "arguments": ["/test_node", "/test_param"]
  },
  "response": {
    "timestamp": "2024-01-01T12:00:00Z",
    "status_code": 1,
    "message": "Parameter found",
    "value": "test_value",
    "function": "getParam"
  }
}
```

## Exit Codes

- `0`: All tests passed (results match)
- `1`: One or more tests failed (results don't match)

## Testing Strategy

### 1. Generate Log File

```bash
# Start your ROS Master with debug logging
RUST_LOG=debug cargo run --release --bin ros-core-rs |& tee output.log

# In another terminal, run your ROS code
rosparam set /run_id 2ffbf8a4-5cb6-11f0-b764-13f1aaf73515
# ... run your ROS nodes and applications
```

### 2. Convert Log to JSONL

```bash
python3 scripts/convert_log_to_json.py output.log output.jsonl
```

### 3. Replay Against Reference

```bash
# Start reference ROS Master
rosmaster

# In another terminal, replay the log
cargo run --bin replay_log -- output.jsonl
```

### 4. Replay Against Your Implementation

```bash
# Start your ROS Master
cargo run --release --bin ros-core-rs

# In another terminal, replay the log
cargo run --bin replay_log -- --target-uri http://localhost:11312 output.jsonl
```

## Advanced Usage

### Filtering by Function

```bash
# Test only parameter operations
cargo run --bin replay_log -- --function-filter getParam output.jsonl

# Test only publisher operations
cargo run --bin replay_log -- --function-filter registerPublisher output.jsonl
```

### Performance Testing

```bash
# Test only first 50 requests
cargo run --bin replay_log -- --max-requests 50 output.jsonl
```

### Verbose Debugging

```bash
# Show detailed results for each request (default)
cargo run --bin replay_log -- output.jsonl

# Suppress detailed output
cargo run --bin replay_log -- --quiet output.jsonl
```

### Message Comparison

```bash
# Include message comparison
cargo run --bin replay_log -- --compare-messages output.jsonl
```

## Troubleshooting

### Common Issues

1. **File Not Found**: Ensure the JSONL file exists and is readable
2. **Connection Refused**: Ensure the target ROS Master is running and accessible
3. **Invalid JSON**: Check that the log file is properly formatted JSONL
4. **Permission Denied**: Ensure you have read access to the log file

### Debugging Mismatches

1. The tool provides detailed output by default (use `--quiet` to suppress)
2. Use `--continue` to see all mismatches instead of stopping at the first one
3. Check the specific error messages and result values in the mismatch details
4. Use `--compare-messages` to include message comparison in the analysis

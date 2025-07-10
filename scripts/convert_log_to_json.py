#!/usr/bin/env python3
"""
Convert ROS core log lines to JSON lines format.

Parses log lines containing function calls and their return values,
extracting timestamp, function name, arguments, status code, message, and value.
"""

import json
import re
import sys
from datetime import datetime
from typing import Any, List, Optional, Tuple

# Global statistics tracking
stats: dict = {
    'total_lines': 0,
    'debug_lines': 0,
    'returns_lines': 0,
    'regex_matches': 0,
    'parse_success': 0,
    'parse_failures': 0,  # lines with any error
    'lines_skipped': 0,   # lines not converted at all
    'errors_by_type': {}
}

def log_error(error_type: str, line_num: int, line: str, details: str = ""):
    """Log detailed error information."""
    global stats
    if error_type not in stats['errors_by_type']:
        stats['errors_by_type'][error_type] = []

    error_info = {
        'line_num': line_num,
        'line': line.strip(),
        'details': details
    }
    stats['errors_by_type'][error_type].append(error_info)
    stats['parse_failures'] += 1  # Increment for any error

    print(f"ERROR [{error_type}] Line {line_num}: {details}", file=sys.stderr)
    if len(line.strip()) > 100:
        print(f"  Line preview: {line.strip()[:100]}...", file=sys.stderr)
    else:
        print(f"  Full line: {line.strip()}", file=sys.stderr)

def parse_timestamp(timestamp_str: str) -> str:
    """Parse timestamp and convert to ISO format."""
    try:
        dt = datetime.fromisoformat(timestamp_str.replace('Z', '+00:00'))
        return dt.isoformat()
    except Exception as e:
        raise ValueError(f"Failed to parse timestamp '{timestamp_str}': {e}")



def extract_function_response(line: str, line_num: int) -> Optional[Tuple[str, list, str, int, str, Any]]:
    """Extract function call and response data from a log line."""
    if ' returns ' not in line:
        log_error("no_returns", line_num, line, "Line does not contain ' returns '")
        return None

    left, _, right = line.partition(' returns ')
    # left: '[timestamp DEBUG ros_core_rs::core] functionName(args)'
    # right: '(status, "message", value)'

    # Extract timestamp and function call
    left_pattern_brackets = r'\[([^\]]+)\s+DEBUG\s+ros_core_rs::core\]\s+([^\[]+)\[(.*)\]'
    left_pattern_parens = r'\[([^\]]+)\s+DEBUG\s+ros_core_rs::core\]\s+([^(]+)\((.*)\)'
    left_match = re.match(left_pattern_brackets, left)
    if not left_match:
        left_match = re.match(left_pattern_parens, left)
    if not left_match:
        log_error("left_parse_failed", line_num, line, "Failed to parse left part")
        return None
    timestamp, function_name, args_str = left_match.groups()

    # Map of function names to expected argument counts
    FUNCTION_ARG_COUNTS = {
        'setParam': 3,
        'getParam': 2,
        'subscribeParam': 3,
        'registerPublisher': 4,
        'registerSubscriber': 4,
        'registerService': 4,
        'unregisterPublisher': 3,
        'unregisterSubscriber': 3,
        'unregisterService': 3,
        'deleteParam': 2,
        'hasParam': 2,
        'searchParam': 2,
        'getParamNames': 1,
        'getPublishedTopics': 2,
        'getSystemState': 1,
        'getPid': 1,
        # Add more as needed
    }

    # Parse arguments as JSON array if possible
    args = []
    if args_str.strip():
        try:
            # If already bracketed, parse directly
            if args_str.strip().startswith('[') and args_str.strip().endswith(']'):
                args = json.loads(args_str.strip())
            else:
                # Fallback: wrap in brackets and try
                args = json.loads(f'[{args_str.strip()}]')
        except Exception as e:
            log_error("args_parse_failed", line_num, line, f"Failed to parse arguments as JSON array: {e}")
            args = [args_str.strip('"\'')]
    else:
        args = []

        # Parse response array: [status, "message", value]
    right = right.strip()

    # Handle both bracket and parenthesis formats
    if right.startswith('[') and right.endswith(']'):
        response_array_str = right
    elif right.startswith('(') and right.endswith(')'):
        # Legacy format with parentheses - convert to brackets
        response_array_str = '[' + right[1:-1] + ']'
    else:
        log_error("response_format_failed", line_num, line, f"Response is not in expected format: {right}")
        return None



    try:
        # Parse the entire response as a JSON array
        response_array = json.loads(response_array_str)

        if not isinstance(response_array, list) or len(response_array) < 3:
            log_error("response_array_invalid", line_num, line, f"Response array must have at least 3 elements: {response_array}")
            return None

        status_code = response_array[0]
        message = response_array[1]
        value = response_array[2]

        # Ensure status_code is an integer
        if not isinstance(status_code, int):
            try:
                status_code = int(status_code)
            except (ValueError, TypeError) as e:
                log_error("status_parse_failed", line_num, line, f"Failed to parse status '{status_code}': {e}")
                status_code = -999

    except json.JSONDecodeError as e:
        log_error("response_json_parse_failed", line_num, line, f"Failed to parse response as JSON: {e}")
        return None

    # Parse timestamp to correct format
    try:
        parsed_timestamp = parse_timestamp(timestamp)
    except Exception as e:
        log_error("timestamp_parse_failed", line_num, line, f"Failed to parse timestamp: {e}")
        parsed_timestamp = timestamp

    return function_name, args, parsed_timestamp, status_code, message, value

def convert_log_lines_to_jsonl(lines: List[str]) -> List[dict]:
    """Convert a list of log lines to a list of parsed JSON dicts."""
    results = []
    global stats
    stats = {
        'total_lines': 0,
        'debug_lines': 0,
        'returns_lines': 0,
        'regex_matches': 0,
        'parse_success': 0,
        'parse_failures': 0,
        'lines_skipped': 0,
        'errors_by_type': {}
    }

    for line_num, line in enumerate(lines, 1):
        line = line.strip()
        stats['total_lines'] += 1
        if not line:
            continue
        if 'DEBUG ros_core_rs::core' in line:
            stats['debug_lines'] += 1
            if 'returns' in line:
                stats['returns_lines'] += 1
                response_data = extract_function_response(line, line_num)
                if response_data:
                    function_name, args, timestamp, status_code, message, value = response_data
                    if isinstance(args, tuple):
                        args = list(args)
                    json_entry = {
                        "request": {
                            "timestamp": timestamp,
                            "function": function_name,
                            "arguments": args
                        },
                        "response": {
                            "timestamp": timestamp,
                            "status_code": status_code if status_code is not None else -999,
                            "message": message if message is not None else "PARSE_ERROR",
                            "value": value
                        }
                    }
                    results.append(json_entry)
                    stats['parse_success'] += 1
                else:
                    stats['lines_skipped'] += 1  # Only increment for total parse failure
    return results

def convert_log_to_json(input_file: str, output_file: str, debug_file: Optional[str] = None):
    """Convert the log file to JSON lines format."""
    with open(input_file, 'r') as f_in:
        lines = f_in.readlines()

    results = convert_log_lines_to_jsonl(lines)

    with open(output_file, 'w') as f_out:
        for entry in results:
            f_out.write(json.dumps(entry) + '\n')

    print(f"\n=== CONVERSION STATISTICS ===")
    print(f"Total lines processed: {stats['total_lines']}")
    print(f"DEBUG lines: {stats['debug_lines']}")
    print(f"Lines with 'returns': {stats['returns_lines']}")
    print(f"Successfully parsed: {stats['parse_success']}")
    print(f"Failed to parse (any error): {stats['parse_failures']}")
    print(f"Lines skipped (not converted): {stats['lines_skipped']}")

    if stats['errors_by_type']:
        print(f"\n=== ERROR BREAKDOWN ===")
        for error_type, errors in stats['errors_by_type'].items():
            print(f"{error_type}: {len(errors)} errors")
            for i, error in enumerate(errors):
                print(f"  Line {error['line_num']}: {error['details']}")
                if len(error['line']) > 100:
                    print(f"    {error['line'][:100]}...")
                else:
                    print(f"    {error['line']}")

    print(f"\nProcessed {len(results)} responses")
    print(f"Failed to parse {stats['parse_failures']} lines")

    if debug_file:
        with open(debug_file, 'w') as f_debug:
            f_debug.write("=== DETAILED ERROR REPORT ===\n")
            for error_type, errors in stats['errors_by_type'].items():
                f_debug.write(f"\n{error_type.upper()} ({len(errors)} errors):\n")
                for error in errors:
                    f_debug.write(f"Line {error['line_num']}: {error['details']}\n")
                    f_debug.write(f"  {error['line']}\n\n")
        print(f"Detailed error report written to: {debug_file}")

if __name__ == "__main__":
    import ast
    if len(sys.argv) < 3 or len(sys.argv) > 4:
        print("Usage: python3 convert_log_to_json.py <input.log> <output.jsonl> [debug_report.txt]")
        sys.exit(1)
    input_file = sys.argv[1]
    output_file = sys.argv[2]
    debug_file = sys.argv[3] if len(sys.argv) == 4 else None
    convert_log_to_json(input_file, output_file, debug_file)
    print(f"Successfully converted {input_file} to {output_file}")

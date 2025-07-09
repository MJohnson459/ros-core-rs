#!/usr/bin/env python3
"""
Enhanced log converter with detailed error reporting and debugging.
Takes the output.log file and converts it into a jsonl file.
Only processes lines from core that have "returns" in the string.
"""

import re
import json
import ast
from typing import Optional, Tuple, Any, Dict
from datetime import datetime
import sys

# Global counters for detailed reporting
stats: Dict[str, Any] = {
    'total_lines': 0,
    'debug_lines': 0,
    'returns_lines': 0,
    'regex_matches': 0,
    'parse_success': 0,
    'parse_failures': 0,
    'errors_by_type': {}
}

def log_error(error_type: str, line_num: int, line: str, details: str = ""):
    """Log detailed error information."""
    if error_type not in stats['errors_by_type']:
        stats['errors_by_type'][error_type] = []

    error_info = {
        'line_num': line_num,
        'line': line.strip(),
        'details': details
    }
    stats['errors_by_type'][error_type].append(error_info)

    # Print to stderr for immediate feedback
    print(f"ERROR [{error_type}] Line {line_num}: {details}", file=sys.stderr)
    if len(line.strip()) > 100:
        print(f"  Line preview: {line.strip()[:100]}...", file=sys.stderr)
    else:
        print(f"  Full line: {line.strip()}", file=sys.stderr)

def parse_timestamp(timestamp_str: str) -> str:
    """Parse timestamp and convert to ISO format."""
    try:
        # Remove timezone info and parse
        dt = datetime.fromisoformat(timestamp_str.replace('Z', '+00:00'))
        return dt.isoformat()
    except Exception as e:
        raise ValueError(f"Failed to parse timestamp '{timestamp_str}': {e}")

def pyify_bools(s: str) -> str:
    """Convert JSON-style booleans and nulls to Python equivalents."""
    s = re.sub(r'\btrue\b', 'True', s, flags=re.IGNORECASE)
    s = re.sub(r'\bfalse\b', 'False', s, flags=re.IGNORECASE)
    s = re.sub(r'\bnull\b', 'None', s, flags=re.IGNORECASE)
    return s

def quote_dict_keys(s: str) -> str:
    """Quote unquoted dictionary keys."""
    # Pattern to match unquoted keys in dictionaries
    pattern = r'(\w+):'
    return re.sub(pattern, r'"\1":', s)

def extract_function_response(line: str, line_num: int) -> Optional[Tuple[str, list, str, int, str, Any]]:
    """Extract all information from a response line using a simpler approach."""
    # Pattern: [timestamp DEBUG ros_core_rs::core] functionName(...) returns (...)
    # First match the basic structure
    pattern = r'\[([^\]]+)\s+DEBUG\s+ros_core_rs::core\]\s+([^(]+)\(.*\)\s+returns\s+\((.+)\)$'
    match = re.match(pattern, line)

    if not match:
        log_error("regex_no_match", line_num, line, "Line does not match expected pattern")
        return None

    stats['regex_matches'] += 1
    timestamp, function_name, response_str = match.groups()

    # Now extract the arguments manually by finding the function call
    # Look for functionName( and find the matching closing parenthesis
    func_start = line.find(f"{function_name}(")
    if func_start == -1:
        log_error("function_not_found", line_num, line, f"Could not find function {function_name}")
        return None

    # Find the opening parenthesis
    open_paren = func_start + len(function_name)
    if line[open_paren] != '(':
        log_error("paren_mismatch", line_num, line, f"Expected '(' after function name")
        return None

    # Find the matching closing parenthesis, handling nested quotes
    paren_count = 0
    in_quotes = False
    quote_char = None
    args_str = ""

    for i in range(open_paren + 1, len(line)):
        char = line[i]

        if char in ['"', "'"]:
            if not in_quotes:
                in_quotes = True
                quote_char = char
            elif char == quote_char:
                in_quotes = False
                quote_char = None
        elif not in_quotes:
            if char == '(':
                paren_count += 1
            elif char == ')':
                if paren_count == 0:
                    # This is the closing parenthesis for our function call
                    break
                paren_count -= 1

        args_str += char

    try:
        # Parse arguments
        args = []
        if args_str.strip():
            try:
                args = ast.literal_eval(args_str)
            except Exception as e:
                # Fallback: try with some preprocessing
                try:
                    prepped = quote_dict_keys(pyify_bools(args_str))
                    args = ast.literal_eval(prepped)
                except Exception as e2:
                    log_error("args_parse_failed", line_num, line,
                             f"Failed to parse arguments '{args_str}': {e}, fallback also failed: {e2}")
                    # Continue with empty args rather than failing completely
                    args = []

        # Parse response tuple using a simpler approach
        # Find the first two commas that are outside of quotes and brackets
        def find_split_points(s):
            splits = []
            paren_count = 0
            bracket_count = 0
            in_quotes = False
            quote_char = None

            for i, char in enumerate(s):
                if char == '"' or char == "'":
                    if not in_quotes:
                        in_quotes = True
                        quote_char = char
                    elif char == quote_char:
                        in_quotes = False
                        quote_char = None
                elif not in_quotes:
                    if char == '(':
                        paren_count += 1
                    elif char == ')':
                        paren_count -= 1
                    elif char == '[':
                        bracket_count += 1
                    elif char == ']':
                        bracket_count -= 1
                    elif char == ',' and paren_count == 0 and bracket_count == 0:
                        splits.append(i)
                        if len(splits) == 2:  # We only need the first two splits
                            break

            return splits

        splits = find_split_points(response_str)
        if len(splits) >= 2:
            # Split into three parts
            status_part = response_str[:splits[0]].strip()
            message_part = response_str[splits[0]+1:splits[1]].strip()
            value_part = response_str[splits[1]+1:].strip()

            # Parse status (should be a number)
            try:
                status_code = int(status_part)
            except ValueError as e:
                log_error("status_parse_failed", line_num, line,
                         f"Failed to parse status '{status_part}': {e}")
                status_code = -999

            # Parse message (remove quotes if present)
            message = message_part.strip('"\'')

            # Parse value (could be complex)
            try:
                # Try to parse as Python literal, with boolean conversion
                value = ast.literal_eval(pyify_bools(value_part))
            except Exception as e:
                # If that fails, treat as string
                value = value_part.strip('"\'')
                log_error("value_parse_failed", line_num, line,
                         f"Failed to parse value '{value_part}', treating as string: {e}")
        else:
            # Fallback: try to parse the whole tuple
            try:
                prepped = quote_dict_keys(pyify_bools(f"[{response_str}]"))
                response_list = ast.literal_eval(prepped)

                if response_list and isinstance(response_list[0], tuple):
                    response_tuple = response_list[0]
                    status_code = int(response_tuple[0])
                    message = response_tuple[1]
                    value = response_tuple[2] if len(response_tuple) > 2 else None
                else:
                    status_code = int(response_list[0])
                    message = response_list[1]
                    value = response_list[2] if len(response_list) > 2 else None
            except Exception as e:
                log_error("fallback_parse_failed", line_num, line,
                         f"Both split parsing and fallback parsing failed: {e}")
                return None

        # Parse timestamp
        try:
            parsed_timestamp = parse_timestamp(timestamp)
        except Exception as e:
            log_error("timestamp_parse_failed", line_num, line, f"Failed to parse timestamp: {e}")
            parsed_timestamp = timestamp  # Use original as fallback

        return function_name, args, parsed_timestamp, status_code, message, value

    except Exception as e:
        log_error("general_parse_failed", line_num, line, f"Unexpected error during parsing: {e}")
        return None

def convert_log_to_json(input_file: str, output_file: str, debug_file: Optional[str] = None):
    """Convert the log file to JSON lines format using only response lines."""
    processed_count = 0
    error_count = 0

    # Reset stats
    global stats
    stats = {
        'total_lines': 0,
        'debug_lines': 0,
        'returns_lines': 0,
        'regex_matches': 0,
        'parse_success': 0,
        'parse_failures': 0,
        'errors_by_type': {}
    }

    with open(input_file, 'r') as f_in, open(output_file, 'w') as f_out:
        for line_num, line in enumerate(f_in, 1):
            line = line.strip()
            stats['total_lines'] += 1

            if not line:
                continue

            # Count DEBUG lines
            if 'DEBUG ros_core_rs::core' in line:
                stats['debug_lines'] += 1

                # Count lines with returns
                if 'returns' in line:
                    stats['returns_lines'] += 1

                    # Try to extract function response (which contains everything)
                    response_data = extract_function_response(line, line_num)
                    if response_data:
                        function_name, args, timestamp, status_code, message, value = response_data

                        # Create JSON entry with both request and response from the same line
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

                        # Write JSON line
                        f_out.write(json.dumps(json_entry) + '\n')
                        processed_count += 1
                        stats['parse_success'] += 1
                    else:
                        error_count += 1
                        stats['parse_failures'] += 1

    # Print detailed statistics
    print(f"\n=== CONVERSION STATISTICS ===")
    print(f"Total lines processed: {stats['total_lines']}")
    print(f"DEBUG lines: {stats['debug_lines']}")
    print(f"Lines with 'returns': {stats['returns_lines']}")
    print(f"Regex matches: {stats['regex_matches']}")
    print(f"Successfully parsed: {stats['parse_success']}")
    print(f"Failed to parse: {stats['parse_failures']}")

    print(f"\n=== ERROR BREAKDOWN ===")
    for error_type, errors in stats['errors_by_type'].items():
        print(f"{error_type}: {len(errors)} errors")
        # Show first few examples of each error type
        for i, error in enumerate(errors[:3]):
            print(f"  Example {i+1} (Line {error['line_num']}): {error['details']}")
        if len(errors) > 3:
            print(f"  ... and {len(errors) - 3} more")

    # Write detailed error report to file if requested
    if debug_file:
        with open(debug_file, 'w') as f_debug:
            f_debug.write("=== DETAILED ERROR REPORT ===\n")
            for error_type, errors in stats['errors_by_type'].items():
                f_debug.write(f"\n{error_type.upper()} ({len(errors)} errors):\n")
                for error in errors:
                    f_debug.write(f"Line {error['line_num']}: {error['details']}\n")
                    f_debug.write(f"  {error['line']}\n\n")

    print(f"\nProcessed {processed_count} responses")
    print(f"Failed to parse {error_count} lines")

    if debug_file:
        print(f"Detailed error report written to: {debug_file}")

if __name__ == "__main__":
    import sys
    if len(sys.argv) < 3 or len(sys.argv) > 4:
        print("Usage: python3 convert_log_to_json.py <input.log> <output.jsonl> [debug_report.txt]")
        sys.exit(1)

    input_file = sys.argv[1]
    output_file = sys.argv[2]
    debug_file = sys.argv[3] if len(sys.argv) == 4 else None

    convert_log_to_json(input_file, output_file, debug_file)
    print(f"Successfully converted {input_file} to {output_file}")

#!/usr/bin/env python3
"""
Test suite for the log conversion script.

Tests various edge cases and parsing scenarios to ensure robust log parsing.
"""

import json
from convert_log_to_json import extract_function_response, convert_log_lines_to_jsonl


def create_test_result(function_name, args, timestamp, status_code, message, value):
    """Create a test result dictionary from parsed function response."""
    if isinstance(args, tuple):
        args = list(args)

    return {
        "request": {
            "timestamp": timestamp,
            "function": function_name,
            "arguments": args
        },
        "response": {
            "timestamp": timestamp,
            "status_code": status_code,
            "message": message,
            "value": value
        }
    }


def print_test_result(test_name, passed, expected=None, actual=None):
    """Print formatted test result."""
    if passed:
        print("  PASSED")
    else:
        print("  FAILED:")
        if expected is not None and actual is not None:
            print(f"    Expected: {json.dumps(expected, indent=2)}")
            print(f"    Actual:   {json.dumps(actual, indent=2)}")


def get_test_cases():
    """Get all test cases for the log conversion functionality."""
    return [
        # Simple string parameter
        {
            "name": "simple_setParam_string",
            "input": '[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] setParam["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"] returns [1, "parameter /run_id set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "function": "setParam",
                    "arguments": ["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "status_code": 1,
                    "message": "parameter /run_id set",
                    "value": 0
                }
            }
        },

        # Dictionary return value
        {
            "name": "subscribeParam_with_dict",
            "input": '[2025-07-09T18:20:59Z DEBUG ros_core_rs::core] subscribeParam["/ln_rst_test_node", "http://LOCLAP858:40873/", "/"] returns [1, "Subscribed to parameter [/]", {"run_id":"2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"}]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:20:59+00:00",
                    "function": "subscribeParam",
                    "arguments": ["/ln_rst_test_node", "http://LOCLAP858:40873/", "/"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:20:59+00:00",
                    "status_code": 1,
                    "message": "Subscribed to parameter [/]",
                    "value": {
                        "run_id": "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"
                    }
                }
            }
        },

        # Error response
        {
            "name": "getParam_error_response",
            "input": '[2025-07-09T18:20:59Z DEBUG ros_core_rs::core] getParam["/ln_rst_test_node", "/use_sim_time"] returns [-1, "/use_sim_time", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:20:59+00:00",
                    "function": "getParam",
                    "arguments": ["/ln_rst_test_node", "/use_sim_time"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:20:59+00:00",
                    "status_code": -1,
                    "message": "/use_sim_time",
                    "value": 0
                }
            }
        },

        # Empty array return value
        {
            "name": "registerPublisher_empty_array",
            "input": '[2025-07-09T18:20:59Z DEBUG ros_core_rs::core] registerPublisher["/ln_rst_test_node", "/rosout", "rosgraph_msgs/Log", "http://LOCLAP858:40873/"] returns [1, "Registered [/ln_rst_test_node] as publisher of [/rosout]", []]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:20:59+00:00",
                    "function": "registerPublisher",
                    "arguments": ["/ln_rst_test_node", "/rosout", "rosgraph_msgs/Log", "http://LOCLAP858:40873/"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:20:59+00:00",
                    "status_code": 1,
                    "message": "Registered [/ln_rst_test_node] as publisher of [/rosout]",
                    "value": []
                }
            }
        },

        # Float return value
        {
            "name": "subscribeParam_float_value",
            "input": '[2025-07-09T18:21:00Z DEBUG ros_core_rs::core] subscribeParam["/central_router", "http://LOCLAP858:40427/", "/central_router/topo_search_time_resolution"] returns [1, "Subscribed to parameter [/central_router/topo_search_time_resolution]", 0.5]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:21:00+00:00",
                    "function": "subscribeParam",
                    "arguments": ["/central_router", "http://LOCLAP858:40427/", "/central_router/topo_search_time_resolution"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:21:00+00:00",
                    "status_code": 1,
                    "message": "Subscribed to parameter [/central_router/topo_search_time_resolution]",
                    "value": 0.5
                }
            }
        },

        # Boolean parameter
        {
            "name": "setParam_boolean_value",
            "input": '[2025-07-09T18:21:00Z DEBUG ros_core_rs::core] setParam["/roslaunch", "/resource_manager/wait_queue_occupants", true] returns [1, "parameter /resource_manager/wait_queue_occupants set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:21:00+00:00",
                    "function": "setParam",
                    "arguments": ["/roslaunch", "/resource_manager/wait_queue_occupants", True]
                },
                "response": {
                    "timestamp": "2025-07-09T18:21:00+00:00",
                    "status_code": 1,
                    "message": "parameter /resource_manager/wait_queue_occupants set",
                    "value": 0
                }
            }
        },

        # Complex array parameter
        {
            "name": "setParam_complex_array",
            "input": '[2025-07-09T18:21:01Z DEBUG ros_core_rs::core] setParam["/roslaunch", "/sim_001/cmd_vel_mux/topics", [{"name":"navigation","timeout":0.5,"topic":"nav/cmd_vel_depth_limited","priority":10},{"name":"object_detection","topic":"nav/cmd_vel_hazard_sighting_limited","timeout":0.25,"priority":11}]] returns [1, "parameter /sim_001/cmd_vel_mux/topics set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:21:01+00:00",
                    "function": "setParam",
                    "arguments": ["/roslaunch", "/sim_001/cmd_vel_mux/topics", [{"name": "navigation", "timeout": 0.5, "topic": "nav/cmd_vel_depth_limited", "priority": 10}, {"name": "object_detection", "topic": "nav/cmd_vel_hazard_sighting_limited", "timeout": 0.25, "priority": 11}]]
                },
                "response": {
                    "timestamp": "2025-07-09T18:21:01+00:00",
                    "status_code": 1,
                    "message": "parameter /sim_001/cmd_vel_mux/topics set",
                    "value": 0
                }
            }
        },

        # XML content
        {
            "name": "setParam_xml_content",
            "input": '[2025-07-09T18:21:01Z DEBUG ros_core_rs::core] setParam["/roslaunch", "/sim_001/robot_description", "<?xml version=1.0 ?><robot name=r1>  <link name=base_link/></robot>"] returns [1, "parameter /sim_001/robot_description set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:21:01+00:00",
                    "function": "setParam",
                    "arguments": ["/roslaunch", "/sim_001/robot_description", "<?xml version=1.0 ?><robot name=r1>  <link name=base_link/></robot>"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:21:01+00:00",
                    "status_code": 1,
                    "message": "parameter /sim_001/robot_description set",
                    "value": 0
                }
            }
        },

        # Empty arguments
        {
            "name": "empty_arguments",
            "input": '[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] getPid["/roslaunch"] returns [1, "", 809617]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "function": "getPid",
                    "arguments": ["/roslaunch"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "status_code": 1,
                    "message": "",
                    "value": 809617
                }
            }
        },

        # Nested quotes in arguments
        {
            "name": "nested_quotes",
            "input": '[2025-07-09T18:21:00Z DEBUG ros_core_rs::core] setParam["/roslaunch", "/test/param", "value with \\"quotes\\" inside"] returns [1, "parameter /test/param set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:21:00+00:00",
                    "function": "setParam",
                    "arguments": ["/roslaunch", "/test/param", 'value with "quotes" inside']
                },
                "response": {
                    "timestamp": "2025-07-09T18:21:00+00:00",
                    "status_code": 1,
                    "message": "parameter /test/param set",
                    "value": 0
                }
            }
        },

        # Request only (no return) - this should fail to parse since no returns
        {
            "name": "request_only",
            "input": '[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] setParam["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"]',
            "expected": None  # Should fail to parse since no returns
        },

        # Return only (no request)
        {
            "name": "return_only",
            "input": '[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] setParam["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"] returns [1, "parameter /run_id set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "function": "setParam",
                    "arguments": ["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "status_code": 1,
                    "message": "parameter /run_id set",
                    "value": 0
                }
            }
        },

        # Legacy format with parentheses (backward compatibility)
        {
            "name": "legacy_parentheses_format",
            "input": '[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] setParam("/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515") returns (1, "parameter /run_id set", 0)',
            "expected": {
                "request": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "function": "setParam",
                    "arguments": ["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"]
                },
                "response": {
                    "timestamp": "2025-07-09T18:20:55+00:00",
                    "status_code": 1,
                    "message": "parameter /run_id set",
                    "value": 0
                }
            }
        },

        # Error case with commas in quoted strings (from debug report)
        {
            "name": "searchParam_error_with_commas_in_message",
            "input": '[2025-07-10T15:56:52Z DEBUG ros_core_rs::core] searchParam["/sim_001/robot_state_publisher_LOCLAP858_417645_2125208095934196923", "tf_prefix"] returns [-1, "Parameter [\\"/sim_001/robot_state_publisher_LOCLAP858_417645_2125208095934196923\\", \\"tf_prefix\\"] is not set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-10T15:56:52+00:00",
                    "function": "searchParam",
                    "arguments": ["/sim_001/robot_state_publisher_LOCLAP858_417645_2125208095934196923", "tf_prefix"]
                },
                "response": {
                    "timestamp": "2025-07-10T15:56:52+00:00",
                    "status_code": -1,
                    "message": "Parameter [\"/sim_001/robot_state_publisher_LOCLAP858_417645_2125208095934196923\", \"tf_prefix\"] is not set",
                    "value": 0
                }
            }
        },

        # Another error case with different parameter
        {
            "name": "searchParam_error_dock_state_publisher",
            "input": '[2025-07-10T15:56:52Z DEBUG ros_core_rs::core] searchParam["/sim_001/dock_state_publisher", "tf_prefix"] returns [-1, "Parameter [\\"/sim_001/dock_state_publisher\\", \\"tf_prefix\\"] is not set", 0]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-10T15:56:52+00:00",
                    "function": "searchParam",
                    "arguments": ["/sim_001/dock_state_publisher", "tf_prefix"]
                },
                "response": {
                    "timestamp": "2025-07-10T15:56:52+00:00",
                    "status_code": -1,
                    "message": "Parameter [\"/sim_001/dock_state_publisher\", \"tf_prefix\"] is not set",
                    "value": 0
                }
            }
        },

        # Test for lookupService with two arguments (previously failed case)
        {
            "name": "lookupService_two_args",
            "input": '[2025-07-10T15:56:51Z DEBUG ros_core_rs::core] lookupService["/monitor_tycho_bridge", "/tycho/get_floor_data"] returns [1, "", "rosrpc://LOCLAP858:43753"]',
            "expected": {
                "request": {
                    "timestamp": "2025-07-10T15:56:51+00:00",
                    "function": "lookupService",
                    "arguments": ["/monitor_tycho_bridge", "/tycho/get_floor_data"]
                },
                "response": {
                    "timestamp": "2025-07-10T15:56:51+00:00",
                    "status_code": 1,
                    "message": "",
                    "value": "rosrpc://LOCLAP858:43753"
                }
            }
        }
    ]


def get_edge_cases():
    """Get edge cases for testing error handling."""
    return [
        {
            "name": "malformed_line",
            "input": "This is not a valid log line",
            "should_fail": True
        },
        {
            "name": "no_returns_keyword",
            "input": "[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] setParam(\"/test\", \"value\")",
            "should_fail": True
        },
        {
            "name": "empty_function_name",
            "input": "[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] () returns (1, 'test', 0)",
            "should_fail": True
        }
    ]


def test_extract_function_response():
    """Test the extract_function_response function with various inputs."""
    test_cases = get_test_cases()

    for test_case in test_cases:
        print(f"\nTesting: {test_case['name']}")

        result = extract_function_response(test_case['input'], 1)

        # Handle case where we expect None (should fail to parse)
        if test_case['expected'] is None:
            passed = result is None
            print_test_result(test_case['name'], passed)
            continue

        if result is None:
            print_test_result(test_case['name'], False)
            continue

        function_name, args, timestamp, status_code, message, value = result
        actual = create_test_result(function_name, args, timestamp, status_code, message, value)

        passed = actual == test_case['expected']
        print_test_result(test_case['name'], passed, test_case['expected'], actual)


def test_convert_log_to_json():
    """Test the full conversion function with a small test file."""
    test_lines = [
        '[2025-07-09T18:20:55Z DEBUG ros_core_rs::core] setParam["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"] returns [1, "parameter /run_id set", 0]',
        '[2025-07-09T18:20:55Z INFO ros_core_rs::core] Some other log message',
        '[2025-07-09T18:20:59Z DEBUG ros_core_rs::core] subscribeParam["/ln_rst_test_node", "http://LOCLAP858:40873/", "/"] returns [1, "Subscribed to parameter [/]", {"run_id":"2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"}]',
        '',
        '[2025-07-09T18:20:59Z DEBUG ros_core_rs::core] getParam["/ln_rst_test_node", "/use_sim_time"] returns [-1, "/use_sim_time", 0]'
    ]

    results = convert_log_lines_to_jsonl(test_lines)

    assert len(results) == 3, f"Expected 3 results, got {len(results)}"

    first_result = results[0]
    expected_first = {
        "request": {
            "timestamp": "2025-07-09T18:20:55+00:00",
            "function": "setParam",
            "arguments": ["/rosparam-1089336", "/run_id", "2ffbf8a4-5cb6-11f0-b764-13f1aaf73515"]
        },
        "response": {
            "timestamp": "2025-07-09T18:20:55+00:00",
            "status_code": 1,
            "message": "parameter /run_id set",
            "value": 0
        }
    }

    assert first_result == expected_first, f"First result mismatch: {first_result}"
    print("  PASSED: Full conversion test")


def test_edge_cases():
    """Test various edge cases that might cause parsing failures."""
    edge_cases = get_edge_cases()

    for test_case in edge_cases:
        print(f"\nTesting edge case: {test_case['name']}")

        result = extract_function_response(test_case['input'], 1)

        if test_case['should_fail']:
            passed = result is None
            print_test_result(test_case['name'], passed)
        else:
            passed = result is not None
            print_test_result(test_case['name'], passed)


def run_tests():
    """Run all tests."""
    print("Running log conversion tests...")

    print("\n=== Testing extract_function_response ===")
    test_extract_function_response()

    print("\n=== Testing full conversion ===")
    test_convert_log_to_json()

    print("\n=== Testing edge cases ===")
    test_edge_cases()

    print("\n=== Test suite completed ===")


if __name__ == "__main__":
    run_tests()

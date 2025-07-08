#!/bin/bash

# ROS Master Comparison Script
# This script runs the comparison tool against different endpoints

# Don't exit on error, we want to continue through all endpoints
# set -e

# Default values
REFERENCE_URI=${REFERENCE_URI:-"http://localhost:11312"}
TEST_URI=${TEST_URI:-"http://localhost:11311"}
VERBOSE=${VERBOSE:-false}
CONTINUE_ON_MISMATCH=${CONTINUE_ON_MISMATCH:-false}

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to run comparison for a single endpoint
run_endpoint_test() {
    local endpoint=$1
    print_status "Testing endpoint: $endpoint"

    local cmd="cargo run --bin ros_master_comparison -- \
        --reference-uri $REFERENCE_URI \
        --test-uri $TEST_URI \
        --endpoint $endpoint \
        --continue-on-mismatch"

    if [ "$VERBOSE" = "true" ]; then
        cmd="$cmd --verbose"
    fi

    # Capture output and run the command
    local output
    local exit_code
    output=$(eval $cmd 2>&1)
    exit_code=$?

    # Display the output
    echo "$output"

    if [ $exit_code -eq 0 ]; then
        print_success "Endpoint $endpoint passed"
        return 0
    else
        print_error "Endpoint $endpoint failed"

        # Show failure summary if there were mismatches
        local mismatches=$(echo "$output" | grep -A 10 "Mismatches" | grep -E "(Reference Result|Test Result|Reference Error|Test Error)" || true)
        if [ -n "$mismatches" ]; then
            echo
            print_error "FAILURE DETAILS:"
            echo "$mismatches" | while IFS= read -r line; do
                if [[ "$line" =~ ^[[:space:]]*Reference ]]; then
                    echo -e "  ${BLUE}$line${NC}"
                elif [[ "$line" =~ ^[[:space:]]*Test ]]; then
                    echo -e "  ${YELLOW}$line${NC}"
                else
                    echo "  $line"
                fi
            done
        fi
        return 1
    fi
}

# Function to run all endpoint tests
run_all_tests() {
    local endpoints=(
        "register-service"
        "un-register-service"
        "register-subscriber"
        "unregister-subscriber"
        "register-publisher"
        "unregister-publisher"
        "lookup-node"
        "get-published-topics"
        "get-topic-types"
        "get-system-state"
        "get-uri"
        "lookup-service"
        "delete-param"
        "set-param"
        "get-param"
        "search-param"
        "subscribe-param"
        "unsubscribe-param"
        "has-param"
        "get-param-names"
        "get-pid"
    )

    local failed_endpoints=()
    local total_endpoints=${#endpoints[@]}
    local passed_count=0
    local failure_details=()

    print_status "Starting comparison tests..."
    print_status "Reference URI: $REFERENCE_URI"
    print_status "Test URI: $TEST_URI"
    echo

            for endpoint in "${endpoints[@]}"; do
        print_status "Testing endpoint: $endpoint"

        # Capture the output and exit code
        local output
        local exit_code
                output=$(cargo run --bin ros_master_comparison -- \
            --reference-uri "$REFERENCE_URI" \
            --test-uri "$TEST_URI" \
            --endpoint "$endpoint" \
            --continue-on-mismatch \
            --verbose 2>&1)
        exit_code=$?

        if [ $exit_code -eq 0 ]; then
            print_success "Endpoint $endpoint passed"
            ((passed_count++))
        else
            print_error "Endpoint $endpoint failed"
            failed_endpoints+=("$endpoint")
            # Extract mismatch details from the output
            local mismatches=$(echo "$output" | grep -A 10 "Mismatches" | grep -E "(Reference Result|Test Result|Reference Error|Test Error)" || true)
            if [ -n "$mismatches" ]; then
                failure_details+=("$endpoint:$mismatches")
            fi
        fi
        echo
    done

    # Summary
    echo "=========================================="
    print_status "Test Summary:"
    echo "  Total endpoints: $total_endpoints"
    echo "  Passed: $passed_count"
    echo "  Failed: $(($total_endpoints - $passed_count))"

    if [ ${#failed_endpoints[@]} -gt 0 ]; then
        echo
        print_error "FAILED ENDPOINTS SUMMARY:"
        echo "=========================================="

        for endpoint in "${failed_endpoints[@]}"; do
            print_error "❌ $endpoint"

            # Find and display the failure details for this endpoint
            for detail in "${failure_details[@]}"; do
                if [[ "$detail" == "$endpoint:"* ]]; then
                    local details="${detail#$endpoint:}"
                    echo "$details" | while IFS= read -r line; do
                        if [[ "$line" =~ ^[[:space:]]*Reference ]]; then
                            echo -e "  ${BLUE}$line${NC}"
                        elif [[ "$line" =~ ^[[:space:]]*Test ]]; then
                            echo -e "  ${YELLOW}$line${NC}"
                        else
                            echo "  $line"
                        fi
                    done
                fi
            done
            echo
        done

        return 1
    else
        print_success "All endpoints passed!"
        return 0
    fi
}

# Main script
main() {
    # Check if specific endpoint is provided
    if [ $# -eq 1 ]; then
        local endpoint=$1
        print_status "Running single endpoint test: $endpoint"
        run_endpoint_test "$endpoint"
    else
        print_status "Running all endpoint tests"
        run_all_tests
    fi
}

# Show help
show_help() {
    echo "ROS Master Comparison Script"
    echo ""
    echo "Usage: $0 [endpoint_name]"
    echo ""
    echo "Environment variables:"
    echo "  REFERENCE_URI      Reference ROS Master URI (default: http://localhost:11311)"
    echo "  TEST_URI          Test ROS Master URI (default: http://localhost:11312)"
    echo "  VERBOSE           Enable verbose output (default: false)"
    echo "  CONTINUE_ON_MISMATCH  Continue testing on mismatch (default: false)"
    echo ""
    echo "Examples:"
    echo "  $0                                    # Run all endpoint tests"
    echo "  $0 register-service                  # Test only register-service"
    echo "  REFERENCE_URI=http://localhost:11311 TEST_URI=http://localhost:11312 $0"
    echo "  VERBOSE=true $0 get-param            # Run get-param test with verbose output"
    echo ""
    echo "Available endpoints:"
    echo "  register-service, un-register-service, register-subscriber, unregister-subscriber"
    echo "  register-publisher, unregister-publisher, lookup-node, get-published-topics"
    echo "  get-topic-types, get-system-state, get-uri, lookup-service, delete-param"
    echo "  set-param, get-param, search-param, subscribe-param, unsubscribe-param"
    echo "  has-param, get-param-names, get-pid"
}

# Parse command line arguments
case "${1:-}" in
    -h|--help|help)
        show_help
        exit 0
        ;;
    *)
        main "$@"
        ;;
esac

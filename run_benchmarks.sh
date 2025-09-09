#!/bin/bash

# Comprehensive benchmark runner for sh2perl
# This script runs different types of benchmarks and generates reports

set -e

# Configuration
ITERATIONS=${ITERATIONS:-3}
WARMUP_RUNS=${WARMUP_RUNS:-1}
VERBOSE=${VERBOSE:-1}

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging function
log() {
    local level=$1
    shift
    local message="$*"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    
    case $level in
        INFO)  echo -e "${GREEN}[$timestamp] [INFO] $message${NC}" ;;
        WARN)  echo -e "${YELLOW}[$timestamp] [WARN] $message${NC}" ;;
        ERROR) echo -e "${RED}[$timestamp] [ERROR] $message${NC}" ;;
        DEBUG) echo -e "${BLUE}[$timestamp] [DEBUG] $message${NC}" ;;
        *)     echo "[$timestamp] [$level] $message" ;;
    esac
}

# Check prerequisites
check_prerequisites() {
    log INFO "Checking prerequisites..."
    
    # Check if bash is available
    if ! command -v bash &> /dev/null; then
        log ERROR "bash is not available"
        exit 1
    fi
    
    # Check if perl is available
    if ! command -v perl &> /dev/null; then
        log ERROR "perl is not available"
        exit 1
    fi
    
    # Check if examples directory exists
    if [ ! -d "examples" ]; then
        log ERROR "examples directory not found"
        exit 1
    fi
    
    # Check if examples.pl directory exists
    if [ ! -d "examples.pl" ]; then
        log ERROR "examples.pl directory not found"
        exit 1
    fi
    
    log INFO "Prerequisites check passed"
}

# Create test environment
setup_test_environment() {
    log INFO "Setting up test environment..."
    
    # Create test data if it doesn't exist
    if [ ! -d "benchmark_test_data" ]; then
        log INFO "Creating test data..."
        perl create_test_data.pl
    else
        log INFO "Test data already exists"
    fi
}

# Run simple benchmark
run_simple_benchmark() {
    log INFO "Running simple benchmark..."
    
    if [ -f "simple_benchmark.pl" ]; then
        perl simple_benchmark.pl
    else
        log ERROR "simple_benchmark.pl not found"
        return 1
    fi
}

# Run comprehensive benchmark
run_comprehensive_benchmark() {
    log INFO "Running comprehensive benchmark..."
    
    if [ -f "benchmark_system.pl" ]; then
        perl benchmark_system.pl
    else
        log WARN "benchmark_system.pl not found, falling back to simple benchmark"
        run_simple_benchmark
    fi
}

# Run specific test category
run_category_benchmark() {
    local category=$1
    log INFO "Running benchmark for category: $category"
    
    case $category in
        "simple")
            run_simple_benchmark
            ;;
        "comprehensive")
            run_comprehensive_benchmark
            ;;
        "file_ops")
            log INFO "Running file operations benchmark..."
            perl simple_benchmark.pl 044_find_example 007_cat_EOF 008_simple_backup
            ;;
        "text_processing")
            log INFO "Running text processing benchmark..."
            perl simple_benchmark.pl 015_grep_advanced 016_grep_basic 017_grep_context
            ;;
        "math")
            log INFO "Running mathematical operations benchmark..."
            perl simple_benchmark.pl 051_primes 052_numeric_computations 053_gcd 054_fibonacci
            ;;
        *)
            log ERROR "Unknown category: $category"
            return 1
            ;;
    esac
}

# Generate performance report
generate_report() {
    log INFO "Generating performance report..."
    
    local report_file="benchmark_report_$(date +%Y%m%d_%H%M%S).md"
    
    cat > "$report_file" << EOF
# SH2PERL Benchmark Report

Generated on: $(date)

## Configuration
- Iterations per test: $ITERATIONS
- Warmup runs: $WARMUP_RUNS
- Test environment: $(uname -s) $(uname -r)

## System Information
- OS: $(uname -s)
- Kernel: $(uname -r)
- Architecture: $(uname -m)
- Bash version: $(bash --version | head -n1)
- Perl version: $(perl --version | head -n1)

## Test Results

EOF

    # Add results from the latest benchmark run
    if [ -f "benchmark_results.json" ]; then
        echo "### Latest Benchmark Results" >> "$report_file"
        echo '```json' >> "$report_file"
        cat benchmark_results.json >> "$report_file"
        echo '```' >> "$report_file"
    fi
    
    log INFO "Report generated: $report_file"
}

# Cleanup function
cleanup() {
    log INFO "Cleaning up..."
    
    # Remove temporary files
    rm -f __tmp_test_output.pl
    rm -f temp_*.pl
    rm -f *.tmp
    
    # Optionally clean up test data
    if [ "$CLEANUP_TEST_DATA" = "1" ]; then
        perl create_test_data.pl cleanup
    fi
}

# Main function
main() {
    log INFO "Starting sh2perl benchmark suite..."
    
    # Parse command line arguments
    local category="comprehensive"
    local cleanup_data=false
    
    while [[ $# -gt 0 ]]; do
        case $1 in
            --category)
                category="$2"
                shift 2
                ;;
            --iterations)
                ITERATIONS="$2"
                shift 2
                ;;
            --warmup)
                WARMUP_RUNS="$2"
                shift 2
                ;;
            --cleanup)
                cleanup_data=true
                shift
                ;;
            --help)
                echo "Usage: $0 [OPTIONS]"
                echo "Options:"
                echo "  --category CATEGORY    Run specific category (simple, comprehensive, file_ops, text_processing, math)"
                echo "  --iterations N         Number of iterations per test (default: 3)"
                echo "  --warmup N            Number of warmup runs (default: 1)"
                echo "  --cleanup             Clean up test data after benchmark"
                echo "  --help                Show this help message"
                exit 0
                ;;
            *)
                log ERROR "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    # Set cleanup flag
    if [ "$cleanup_data" = true ]; then
        export CLEANUP_TEST_DATA=1
    fi
    
    # Run benchmark
    check_prerequisites
    setup_test_environment
    
    # Set trap for cleanup
    trap cleanup EXIT
    
    run_category_benchmark "$category"
    generate_report
    
    log INFO "Benchmark suite completed successfully!"
}

# Run main function
main "$@"


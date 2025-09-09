@echo off
setlocal enabledelayedexpansion

REM Comprehensive benchmark runner for sh2perl (Windows version)
REM This script runs different types of benchmarks and generates reports

set ITERATIONS=3
set WARMUP_RUNS=1
set VERBOSE=1

echo Starting sh2perl benchmark suite...

REM Check prerequisites
echo Checking prerequisites...

where bash >nul 2>&1
if %errorlevel% neq 0 (
    echo ERROR: bash is not available
    exit /b 1
)

where perl >nul 2>&1
if %errorlevel% neq 0 (
    echo ERROR: perl is not available
    exit /b 1
)

if not exist "examples" (
    echo ERROR: examples directory not found
    exit /b 1
)

if not exist "examples.pl" (
    echo ERROR: examples.pl directory not found
    exit /b 1
)

echo Prerequisites check passed

REM Setup test environment
echo Setting up test environment...

if not exist "benchmark_test_data" (
    echo Creating test data...
    perl create_test_data.pl
) else (
    echo Test data already exists
)

REM Parse command line arguments
set CATEGORY=comprehensive
set CLEANUP_DATA=0

:parse_args
if "%~1"=="" goto run_benchmark
if "%~1"=="--category" (
    set CATEGORY=%~2
    shift
    shift
    goto parse_args
)
if "%~1"=="--iterations" (
    set ITERATIONS=%~2
    shift
    shift
    goto parse_args
)
if "%~1"=="--warmup" (
    set WARMUP_RUNS=%~2
    shift
    shift
    goto parse_args
)
if "%~1"=="--cleanup" (
    set CLEANUP_DATA=1
    shift
    goto parse_args
)
if "%~1"=="--help" (
    echo Usage: %0 [OPTIONS]
    echo Options:
    echo   --category CATEGORY    Run specific category (simple, comprehensive, file_ops, text_processing, math)
    echo   --iterations N         Number of iterations per test (default: 3)
    echo   --warmup N            Number of warmup runs (default: 1)
    echo   --cleanup             Clean up test data after benchmark
    echo   --help                Show this help message
    exit /b 0
)
shift
goto parse_args

:run_benchmark
echo Running benchmark for category: %CATEGORY%

if "%CATEGORY%"=="simple" (
    call :run_simple_benchmark
) else if "%CATEGORY%"=="comprehensive" (
    call :run_comprehensive_benchmark
) else if "%CATEGORY%"=="file_ops" (
    echo Running file operations benchmark...
    perl simple_benchmark.pl 044_find_example 007_cat_EOF 008_simple_backup
) else if "%CATEGORY%"=="text_processing" (
    echo Running text processing benchmark...
    perl simple_benchmark.pl 015_grep_advanced 016_grep_basic 017_grep_context
) else if "%CATEGORY%"=="math" (
    echo Running mathematical operations benchmark...
    perl simple_benchmark.pl 051_primes 052_numeric_computations 053_gcd 054_fibonacci
) else (
    echo ERROR: Unknown category: %CATEGORY%
    exit /b 1
)

call :generate_report
call :cleanup

echo Benchmark suite completed successfully!
exit /b 0

:run_simple_benchmark
echo Running simple benchmark...
if exist "simple_benchmark.pl" (
    perl simple_benchmark.pl
) else (
    echo ERROR: simple_benchmark.pl not found
    exit /b 1
)
goto :eof

:run_comprehensive_benchmark
echo Running comprehensive benchmark...
if exist "benchmark_system.pl" (
    perl benchmark_system.pl
) else (
    echo WARNING: benchmark_system.pl not found, falling back to simple benchmark
    call :run_simple_benchmark
)
goto :eof

:generate_report
echo Generating performance report...

set REPORT_FILE=benchmark_report_%date:~-4,4%%date:~-10,2%%date:~-7,2%_%time:~0,2%%time:~3,2%%time:~6,2%.md
set REPORT_FILE=%REPORT_FILE: =0%

(
echo # SH2PERL Benchmark Report
echo.
echo Generated on: %date% %time%
echo.
echo ## Configuration
echo - Iterations per test: %ITERATIONS%
echo - Warmup runs: %WARMUP_RUNS%
echo - Test environment: Windows
echo.
echo ## System Information
echo - OS: Windows
echo - Bash version: 
bash --version ^| findstr /C:"GNU bash"
echo - Perl version: 
perl --version ^| findstr /C:"This is perl"
echo.
echo ## Test Results
echo.
) > "%REPORT_FILE%"

if exist "benchmark_results.json" (
    echo ### Latest Benchmark Results >> "%REPORT_FILE%"
    echo ```json >> "%REPORT_FILE%"
    type benchmark_results.json >> "%REPORT_FILE%"
    echo ``` >> "%REPORT_FILE%"
)

echo Report generated: %REPORT_FILE%
goto :eof

:cleanup
echo Cleaning up...

REM Remove temporary files
if exist "__tmp_test_output.pl" del "__tmp_test_output.pl"
if exist "temp_*.pl" del "temp_*.pl"
if exist "*.tmp" del "*.tmp"

REM Optionally clean up test data
if "%CLEANUP_DATA%"=="1" (
    perl create_test_data.pl cleanup
)
goto :eof


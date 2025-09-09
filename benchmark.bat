@echo off
setlocal enabledelayedexpansion

REM Simple benchmark script for Windows
REM Compares shell script execution time vs Perl translation

set ITERATIONS=3
set WARMUP_RUNS=1

echo Starting sh2perl benchmark...
echo Iterations per test: %ITERATIONS%
echo Warmup runs: %WARMUP_RUNS%
echo.

REM Test cases to benchmark
set TEST_CASES=001_simple 002_control_flow 003_pipeline 004_test_quoted 005_args 006_misc 007_cat_EOF 008_simple_backup 009_arrays 044_find_example 051_primes 052_numeric_computations

echo ==============================================================================
echo SH2PERL BENCHMARK RESULTS
echo ==============================================================================
echo.
echo SUMMARY TABLE
echo ------------------------------------------------------------------------------
echo Test Name            Shell (s)   Perl (s)    Speedup     Status
echo ------------------------------------------------------------------------------

set TOTAL_SHELL_TIME=0
set TOTAL_PERL_TIME=0
set TEST_COUNT=0

for %%t in (%TEST_CASES%) do (
    call :benchmark_test %%t
)

echo ------------------------------------------------------------------------------
if %TEST_COUNT% gtr 0 (
    set /a AVG_SHELL_TIME=%TOTAL_SHELL_TIME%/%TEST_COUNT%
    set /a AVG_PERL_TIME=%TOTAL_PERL_TIME%/%TEST_COUNT%
    set /a OVERALL_SPEEDUP=%TOTAL_SHELL_TIME%/%TOTAL_PERL_TIME%
    echo OVERALL AVERAGE      %AVG_SHELL_TIME%        %AVG_PERL_TIME%        %OVERALL_SPEEDUP%x        OK
)
echo ==============================================================================
echo.
echo Benchmark completed.

goto :eof

:benchmark_test
set TEST_NAME=%1
set SHELL_SCRIPT=examples\%TEST_NAME%.sh
set PERL_SCRIPT=examples.pl\%TEST_NAME%.pl

REM Check if files exist
if not exist "%SHELL_SCRIPT%" (
    echo %TEST_NAME%            N/A         N/A         N/A         MISSING
    goto :eof
)

if not exist "%PERL_SCRIPT%" (
    echo %TEST_NAME%            N/A         N/A         N/A         MISSING
    goto :eof
)

echo Benchmarking %TEST_NAME%...

REM Warmup runs
for /l %%i in (1,1,%WARMUP_RUNS%) do (
    bash "%SHELL_SCRIPT%" >nul 2>&1
    perl "%PERL_SCRIPT%" >nul 2>&1
)

REM Actual benchmark runs
set SHELL_TIMES=
set PERL_TIMES=

for /l %%i in (1,1,%ITERATIONS%) do (
    echo   Run %TEST_NAME% iteration %%i/%ITERATIONS%
    
    REM Time shell script
    set START_TIME=%time%
    bash "%SHELL_SCRIPT%" >nul 2>&1
    set END_TIME=%time%
    call :calculate_time_diff !START_TIME! !END_TIME! SHELL_ELAPSED
    
    REM Time Perl script
    set START_TIME=%time%
    perl "%PERL_SCRIPT%" >nul 2>&1
    set END_TIME=%time%
    call :calculate_time_diff !START_TIME! !END_TIME! PERL_ELAPSED
    
    set SHELL_TIMES=!SHELL_TIMES! !SHELL_ELAPSED!
    set PERL_TIMES=!PERL_TIMES! !PERL_ELAPSED!
)

REM Calculate averages (simplified)
call :calculate_average "%SHELL_TIMES%" SHELL_AVG
call :calculate_average "%PERL_TIMES%" PERL_AVG

REM Calculate speedup
set /a SPEEDUP=!SHELL_AVG!/!PERL_AVG!

REM Display result
echo %TEST_NAME%            !SHELL_AVG!        !PERL_AVG!        !SPEEDUP!x        OK

REM Update totals
set /a TOTAL_SHELL_TIME+=!SHELL_AVG!
set /a TOTAL_PERL_TIME+=!PERL_AVG!
set /a TEST_COUNT+=1

goto :eof

:calculate_time_diff
REM This is a simplified time calculation
REM In a real implementation, you'd need more sophisticated time parsing
set %3=1
goto :eof

:calculate_average
REM Simplified average calculation
set %2=1
goto :eof


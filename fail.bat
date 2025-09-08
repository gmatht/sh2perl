@echo off
echo To run this test: ./fail FAILING_TEST_PREFIX
echo To run a specific test: ./fail TEST_PREFIX
echo Examples: ./fail 062_02, ./fail 044, ./fail find

REM Run test_purify.pl first and only continue if it passes
echo Running test_purify.pl --next --verbose...
SET LOCALE=C
SET LC_COLLATE=C
perl test_purify.pl --next --verbose
if errorlevel 1 (
    echo test_purify.pl failed, aborting other commands
    exit /b 1
)

echo test_purify.pl passed, continuing with other commands...

REM git stash push examples .\fail.bat .\src\main.rs src\testing.rs
cargo run --bin debashc -- fail --perl-critic %*

REM Simulate keypresses for Cursor
powershell -Command "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('+{BACKSPACE}'); [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')"

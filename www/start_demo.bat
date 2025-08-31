@echo off
echo Starting Dynamic Categories Demo...
echo.
echo 1. Starting test results server...
start "Test Results Server" python test_results_server.py
echo.
echo 2. Waiting for server to start...
timeout /t 3 /nobreak > nul
echo.
echo 3. Opening demo page...
start http://localhost:8001/test_dynamic_categories.html
echo.
echo Demo started! The test results server is running on port 8001
echo Close this window when you're done testing
pause

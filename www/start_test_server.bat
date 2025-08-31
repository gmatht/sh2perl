@echo off
echo Starting test results server...
cd /d "%~dp0"
python test_results_server.py
pause

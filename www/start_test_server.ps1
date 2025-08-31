# PowerShell script to start the test results server
Write-Host "Starting test results server..." -ForegroundColor Green
Set-Location $PSScriptRoot
python test_results_server.py

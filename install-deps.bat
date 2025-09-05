@echo off
REM install-deps.bat - Simple wrapper for install-deps.ps1
REM This batch file makes it easier to run the PowerShell installer

echo Running sh2perl dependency installer...
echo.

REM Check if PowerShell is available
powershell -Command "Get-Host" >nul 2>&1
if %errorlevel% neq 0 (
    echo Error: PowerShell is not available or not working properly.
    echo Please install PowerShell and try again.
    pause
    exit /b 1
)

REM Run the PowerShell script
powershell -ExecutionPolicy Bypass -File "%~dp0install-deps.ps1" %*

echo.
echo Installation script completed.
pause

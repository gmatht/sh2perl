@echo off
echo Testing Remark42 Server Connection...
echo.

echo 1. Testing direct IP connection...
powershell "Test-NetConnection -ComputerName 81.4.105.17 -Port 80"
echo.

echo 2. Testing domain name connection...
powershell "Test-NetConnection -ComputerName eu.dansted.org -Port 80"
echo.

echo 3. Testing HTTP request...
powershell "try { $response = Invoke-WebRequest -Uri http://eu.dansted.org -UseBasicParsing; Write-Host 'SUCCESS: HTTP' $response.StatusCode } catch { Write-Host 'ERROR:' $_.Exception.Message }"
echo.

echo 4. Opening browser to test page...
start http://81.4.105.17/access.html
echo.

echo 5. Alternative access methods:
echo    - Direct IP: http://81.4.105.17
echo    - Domain: http://eu.dansted.org
echo    - Alternative port: http://eu.dansted.org:8081
echo    - Remark42 Admin: http://eu.dansted.org/remark42/web/
echo.

pause











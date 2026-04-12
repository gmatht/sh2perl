param(
    [int]$TimeoutSeconds = 600,
    [string]$Command = "",
    [string]$Description = "Command"
)

if ($Command -eq "") {
    Write-Host "Usage: ps_timeout.ps1 -TimeoutSeconds <seconds> -Command '<command>' -Description '<description>'"
    exit 1
}

Write-Host "Running: $Description"
Write-Host "Command: $Command"
Write-Host "Timeout: ${TimeoutSeconds}s"
Write-Host "----------------------------------------"

# Start the process
$processInfo = New-Object System.Diagnostics.ProcessStartInfo
$processInfo.FileName = "cmd.exe"
# Convert relative paths to absolute paths
$fullCommand = $Command -replace '^\./', (Join-Path (Get-Location).Path '')
$processInfo.Arguments = "/c $fullCommand"
$processInfo.UseShellExecute = $false
$processInfo.RedirectStandardOutput = $true
$processInfo.RedirectStandardError = $true
$processInfo.CreateNoWindow = $true
$processInfo.WorkingDirectory = (Get-Location).Path

$process = New-Object System.Diagnostics.Process
$process.StartInfo = $processInfo

# Start the process
$process.Start()

# Wait for the process to complete or timeout
$completed = $process.WaitForExit($TimeoutSeconds * 1000)

if (-not $completed) {
    Write-Host "Command timed out after ${TimeoutSeconds}s, killing process..."
    try {
        $process.Kill()
        $process.WaitForExit(5000) # Wait up to 5 seconds for graceful termination
    } catch {
        Write-Host "Failed to kill process gracefully"
    }
    Write-Host "----------------------------------------"
    Write-Host "Command timed out with exit code: 124"
    Write-Host "----------------------------------------"
    exit 124
} else {
    # Process completed normally
    # Read output
    $output = $process.StandardOutput.ReadToEnd()
    $errorOutput = $process.StandardError.ReadToEnd()
    
    # Write output
    if ($output) { Write-Host $output }
    if ($errorOutput) { Write-Host $errorOutput }
    
    Write-Host "----------------------------------------"
    Write-Host "Command completed with exit code: $($process.ExitCode)"
    Write-Host "----------------------------------------"
    exit $process.ExitCode
}

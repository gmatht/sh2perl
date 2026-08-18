# PowerShell benchmark script for sh2perl comparison
# This script provides accurate timing and better Windows compatibility

param(
    [int]$Iterations = 3,
    [int]$WarmupRuns = 1,
    [string[]]$TestCases = @(
        "001_simple",
        "002_control_flow", 
        "003_pipeline",
        "004_test_quoted",
        "005_args",
        "006_misc",
        "007_cat_EOF",
        "008_simple_backup",
        "009_arrays",
        "044_find_example",
        "051_primes",
        "052_numeric_computations"
    )
)

# Global variables
$script:TotalShellTime = 0
$script:TotalPerlTime = 0
$script:TestCount = 0
$script:Results = @()

function Write-Log {
    param([string]$Message)
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    Write-Host "[$timestamp] $Message"
}

function Measure-CommandExecution {
    param(
        [string]$Command,
        [int]$TimeoutSeconds = 30
    )
    
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    
    try {
        $process = Start-Process -FilePath "cmd.exe" -ArgumentList "/c", $Command -PassThru -WindowStyle Hidden -RedirectStandardOutput -RedirectStandardError
        $completed = $process.WaitForExit($TimeoutSeconds * 1000)
        
        if (-not $completed) {
            $process.Kill()
            return @{
                Success = $false
                ElapsedSeconds = $TimeoutSeconds
                Output = "Command timed out after $TimeoutSeconds seconds"
                ExitCode = -1
            }
        }
        
        $stopwatch.Stop()
        $output = $process.StandardOutput.ReadToEnd()
        $error = $process.StandardError.ReadToEnd()
        
        return @{
            Success = $process.ExitCode -eq 0
            ElapsedSeconds = $stopwatch.Elapsed.TotalSeconds
            Output = $output + $error
            ExitCode = $process.ExitCode
        }
    }
    catch {
        $stopwatch.Stop()
        return @{
            Success = $false
            ElapsedSeconds = $stopwatch.Elapsed.TotalSeconds
            Output = $_.Exception.Message
            ExitCode = -1
        }
    }
}

function Test-BenchmarkTest {
    param([string]$TestName)
    
    $shellScript = "examples\$TestName.sh"
    $perlScript = "examples.pl\$TestName.pl"
    
    # Check if files exist
    if (-not (Test-Path $shellScript)) {
        Write-Log "WARNING: Shell script not found: $shellScript"
        return $null
    }
    
    if (-not (Test-Path $perlScript)) {
        Write-Log "WARNING: Perl script not found: $perlScript"
        return $null
    }
    
    Write-Log "Benchmarking $TestName..."
    
    $shellTimes = @()
    $perlTimes = @()
    $shellOutput = ""
    $perlOutput = ""
    
    # Warmup runs
    for ($i = 1; $i -le $WarmupRuns; $i++) {
        Measure-CommandExecution "bash `"$shellScript`"" | Out-Null
        Measure-CommandExecution "perl `"$perlScript`"" | Out-Null
    }
    
    # Actual benchmark runs
    for ($i = 1; $i -le $Iterations; $i++) {
        Write-Log "  Run $TestName iteration $i/$Iterations"
        
        # Test shell script
        $shellResult = Measure-CommandExecution "bash `"$shellScript`""
        if ($shellResult.Success) {
            $shellTimes += $shellResult.ElapsedSeconds
            if ($i -eq 1) { $shellOutput = $shellResult.Output }
        } else {
            Write-Log "  WARNING: Shell script failed with exit code $($shellResult.ExitCode)"
        }
        
        # Test Perl script
        $perlResult = Measure-CommandExecution "perl `"$perlScript`""
        if ($perlResult.Success) {
            $perlTimes += $perlResult.ElapsedSeconds
            if ($i -eq 1) { $perlOutput = $perlResult.Output }
        } else {
            Write-Log "  WARNING: Perl script failed with exit code $($perlResult.ExitCode)"
        }
    }
    
    return @{
        TestName = $TestName
        ShellTimes = $shellTimes
        PerlTimes = $perlTimes
        ShellOutput = $shellOutput
        PerlOutput = $perlOutput
    }
}

function Get-Average {
    param([double[]]$Values)
    
    if ($Values.Count -eq 0) { return 0 }
    return ($Values | Measure-Object -Average).Average
}

function Show-BenchmarkReport {
    param([array]$Results)
    
    Write-Host ""
    Write-Host ("=" * 80)
    Write-Host "SH2PERL BENCHMARK RESULTS"
    Write-Host ("=" * 80)
    Write-Host ""
    
    # Summary table
    Write-Host "SUMMARY TABLE"
    Write-Host ("-" * 80)
    Write-Host ("{0,-20} {1,-12} {2,-12} {3,-12} {4,-12}" -f "Test Name", "Shell (s)", "Perl (s)", "Speedup", "Status")
    Write-Host ("-" * 80)
    
    foreach ($result in $Results) {
        if (-not $result) { continue }
        
        $shellAvg = Get-Average $result.ShellTimes
        $perlAvg = Get-Average $result.PerlTimes
        
        if ($shellAvg -eq 0 -or $perlAvg -eq 0) { continue }
        
        $speedup = [math]::Round($shellAvg / $perlAvg, 2)
        $status = "OK"
        
        # Check if outputs are similar (basic comparison)
        if ($result.ShellOutput -ne $result.PerlOutput) {
            $status = "DIFF"
        }
        
        Write-Host ("{0,-20} {1,-12:F4} {2,-12:F4} {3,-12:F2}x {4,-12}" -f 
                   $result.TestName, $shellAvg, $perlAvg, $speedup, $status)
        
        $script:TotalShellTime += $shellAvg
        $script:TotalPerlTime += $perlAvg
        $script:TestCount++
    }
    
    Write-Host ("-" * 80)
    if ($script:TestCount -gt 0) {
        $avgShellTime = [math]::Round($script:TotalShellTime / $script:TestCount, 4)
        $avgPerlTime = [math]::Round($script:TotalPerlTime / $script:TestCount, 4)
        $overallSpeedup = [math]::Round($script:TotalShellTime / $script:TotalPerlTime, 2)
        
        Write-Host ("{0,-20} {1,-12:F4} {2,-12:F4} {3,-12:F2}x {4,-12}" -f 
                   "OVERALL AVERAGE", $avgShellTime, $avgPerlTime, $overallSpeedup, "OK")
    }
    Write-Host ("=" * 80)
    Write-Host ""
    
    # Detailed results
    Write-Host "DETAILED RESULTS"
    Write-Host ("=" * 80)
    Write-Host ""
    
    foreach ($result in $Results) {
        if (-not $result) { continue }
        
        Write-Host "Test: $($result.TestName)"
        Write-Host ("-" * 50)
        
        $shellAvg = Get-Average $result.ShellTimes
        $perlAvg = Get-Average $result.PerlTimes
        
        if ($shellAvg -gt 0) {
            Write-Host "Shell Script Performance:"
            Write-Host "  Average: $([math]::Round($shellAvg, 4)) seconds"
            $shellTimesStr = ($result.ShellTimes | ForEach-Object { [math]::Round($_, 4) }) -join ", "
            Write-Host "  Times: $shellTimesStr"
        }
        
        if ($perlAvg -gt 0) {
            Write-Host "Perl Script Performance:"
            Write-Host "  Average: $([math]::Round($perlAvg, 4)) seconds"
            $perlTimesStr = ($result.PerlTimes | ForEach-Object { [math]::Round($_, 4) }) -join ", "
            Write-Host "  Times: $perlTimesStr"
        }
        
        if ($shellAvg -gt 0 -and $perlAvg -gt 0) {
            $speedup = [math]::Round($shellAvg / $perlAvg, 2)
            Write-Host "Performance Comparison:"
            $comparison = if ($speedup -gt 1) { "faster" } else { "slower" }
            Write-Host "  Perl is $speedup x $comparison than shell"
        }
        
        # Show output differences if any
        if ($result.ShellOutput -ne $result.PerlOutput) {
            Write-Host "Output Differences Detected:"
            Write-Host "  Shell output length: $($result.ShellOutput.Length) chars"
            Write-Host "  Perl output length: $($result.PerlOutput.Length) chars"
        }
        
        Write-Host ""
    }
}

# Main execution
Write-Log "Starting sh2perl benchmark..."
Write-Log "Iterations per test: $Iterations"
Write-Log "Warmup runs: $WarmupRuns"

$script:Results = @()

foreach ($testName in $TestCases) {
    $result = Test-BenchmarkTest $testName
    $script:Results += $result
}

Show-BenchmarkReport $script:Results

Write-Log "Benchmark completed."

# Save results to JSON file
$resultsFile = "benchmark_results_$(Get-Date -Format 'yyyyMMdd_HHmmss').json"
$script:Results | ConvertTo-Json -Depth 3 | Out-File -FilePath $resultsFile -Encoding UTF8
Write-Log "Results saved to: $resultsFile"


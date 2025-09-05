# install-deps.ps1 - Install dependencies for sh2perl project on Windows
# This script installs Rust, Perl (Strawberry), and Perl::Critic

param(
    [switch]$InstallRust,
    [switch]$InstallPerl,
    [switch]$InstallPerlCritic,
    [switch]$RunBuild,
    [switch]$Force,
    [switch]$SkipRust,
    [switch]$SkipPerl,
    [switch]$SkipPerlCritic
)

# Function to check if running as administrator
function Test-Administrator {
    $currentUser = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($currentUser)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Function to request administrator privileges
function Request-Administrator {
    if (-not (Test-Administrator)) {
        Write-Host "This script requires administrator privileges to install dependencies." -ForegroundColor Yellow
        Write-Host "Requesting administrator privileges..." -ForegroundColor Yellow
        
        $arguments = "-ExecutionPolicy Bypass -File `"$($MyInvocation.MyCommand.Path)`""
        if ($InstallRust) { $arguments += " -InstallRust" }
        if ($InstallPerl) { $arguments += " -InstallPerl" }
        if ($InstallPerlCritic) { $arguments += " -InstallPerlCritic" }
        if ($RunBuild) { $arguments += " -RunBuild" }
        if ($Force) { $arguments += " -Force" }
        
        Start-Process PowerShell -Verb RunAs -ArgumentList $arguments
        exit
    }
}

# Function to check if a command exists
function Test-Command {
    param([string]$Command)
    try {
        Get-Command $Command -ErrorAction Stop | Out-Null
        return $true
    } catch {
        return $false
    }
}

# Function to download and install Rust
function Install-Rust {
    Write-Host "Installing Rust..." -ForegroundColor Green
    
    if (Test-Command "rustc") {
        Write-Host "Rust is already installed." -ForegroundColor Yellow
        return
    }
    
    try {
        # Download and run rustup installer
        $rustupUrl = "https://win.rustup.rs/x86_64"
        $rustupPath = "$env:TEMP\rustup-init.exe"
        
        Write-Host "Downloading Rust installer..." -ForegroundColor Cyan
        Invoke-WebRequest -Uri $rustupUrl -OutFile $rustupPath -UseBasicParsing
        
        Write-Host "Running Rust installer..." -ForegroundColor Cyan
        Start-Process -FilePath $rustupPath -ArgumentList "-y" -Wait
        
        # Add Rust to PATH for current session
        $env:PATH += ";$env:USERPROFILE\.cargo\bin"
        
        Write-Host "Rust installed successfully!" -ForegroundColor Green
    } catch {
        Write-Error "Failed to install Rust: $_"
        return $false
    }
    
    return $true
}

# Function to download and install Strawberry Perl
function Install-StrawberryPerl {
    Write-Host "Installing Strawberry Perl..." -ForegroundColor Green
    
    if (Test-Command "perl") {
        Write-Host "Perl is already installed." -ForegroundColor Yellow
        return
    }
    
    try {
        # Download Strawberry Perl installer
        $perlUrl = "https://strawberryperl.com/download/5.32.1.1/strawberry-perl-5.32.1.1-64bit.msi"
        $perlPath = "$env:TEMP\strawberry-perl.msi"
        
        Write-Host "Downloading Strawberry Perl installer..." -ForegroundColor Cyan
        Invoke-WebRequest -Uri $perlUrl -OutFile $perlPath -UseBasicParsing
        
        Write-Host "Installing Strawberry Perl..." -ForegroundColor Cyan
        Start-Process -FilePath "msiexec.exe" -ArgumentList "/i `"$perlPath`" /quiet" -Wait
        
        # Add Perl to PATH for current session
        $env:PATH += ";C:\Strawberry\perl\bin;C:\Strawberry\perl\site\bin"
        
        Write-Host "Strawberry Perl installed successfully!" -ForegroundColor Green
    } catch {
        Write-Error "Failed to install Strawberry Perl: $_"
        return $false
    }
    
    return $true
}

# Function to install Perl::Critic
function Install-PerlCritic {
    Write-Host "Installing Perl::Critic..." -ForegroundColor Green
    
    if (Test-Command "perlcritic") {
        Write-Host "Perl::Critic is already installed." -ForegroundColor Yellow
        return
    }
    
    try {
        # Use cpan to install Perl::Critic
        Write-Host "Installing Perl::Critic via CPAN..." -ForegroundColor Cyan
        Start-Process -FilePath "C:\Strawberry\perl\bin\cpan.bat" -ArgumentList "Perl::Critic" -Wait -NoNewWindow
        
        Write-Host "Perl::Critic installed successfully!" -ForegroundColor Green
    } catch {
        Write-Error "Failed to install Perl::Critic: $_"
        return $false
    }
    
    return $true
}

# Function to run cargo build
function Invoke-CargoBuild {
    Write-Host "Running cargo build..." -ForegroundColor Green
    
    try {
        Set-Location $PSScriptRoot
        cargo build
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Build completed successfully!" -ForegroundColor Green
        } else {
            Write-Error "Build failed with exit code $LASTEXITCODE"
            return $false
        }
    } catch {
        Write-Error "Failed to run cargo build: $_"
        return $false
    }
    
    return $true
}

# Main execution
Write-Host "=== sh2perl Dependency Installer ===" -ForegroundColor Cyan
Write-Host ""

# Determine what to install (default to all if no specific flags)
$installRust = $InstallRust -or (-not $SkipRust -and -not $InstallPerl -and -not $InstallPerlCritic -and -not $RunBuild)
$installPerl = $InstallPerl -or (-not $SkipPerl -and -not $InstallRust -and -not $InstallPerlCritic -and -not $RunBuild)
$installPerlCritic = $InstallPerlCritic -or (-not $SkipPerlCritic -and -not $InstallRust -and -not $InstallPerl -and -not $RunBuild)

# If no specific flags are provided, install everything
if (-not $InstallRust -and -not $InstallPerl -and -not $InstallPerlCritic -and -not $RunBuild -and -not $SkipRust -and -not $SkipPerl -and -not $SkipPerlCritic) {
    $installRust = $true
    $installPerl = $true
    $installPerlCritic = $true
}

# Check if we need administrator privileges
$needsAdmin = $installPerl -or $installPerlCritic
if ($needsAdmin) {
    Request-Administrator
}

# Install Rust if requested
if ($installRust) {
    Install-Rust
    Write-Host ""
}

# Install Perl if requested
if ($installPerl) {
    Install-StrawberryPerl
    Write-Host ""
}

# Install Perl::Critic if requested
if ($installPerlCritic) {
    Install-PerlCritic
    Write-Host ""
}

# Run build if requested
if ($RunBuild) {
    Invoke-CargoBuild
    Write-Host ""
}

Write-Host "=== Installation Complete ===" -ForegroundColor Green
Write-Host "You may need to restart your terminal or IDE for PATH changes to take effect." -ForegroundColor Yellow

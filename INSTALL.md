# Installation Guide for sh2perl on Windows

This guide explains how to install all dependencies for the sh2perl project on Windows.

## Quick Start

Run the automated installer:

```powershell
.\install-deps.ps1
```

This will install Rust, Strawberry Perl, and Perl::Critic automatically.

## Manual Installation

If you prefer to install dependencies manually or the automated script fails:

### 1. Install Rust

1. Download Rust from https://rustup.rs/
2. Run the installer and follow the prompts
3. Restart your terminal

### 2. Install Strawberry Perl

1. Download Strawberry Perl from https://strawberryperl.com/
2. Run the installer as Administrator
3. Add `C:\Strawberry\perl\bin` and `C:\Strawberry\perl\site\bin` to your PATH

### 3. Install Perl::Critic

1. Open a command prompt as Administrator
2. Run: `C:\Strawberry\perl\bin\cpan.bat Perl::Critic`

### 4. Build the Project

```bash
cargo build
```

## Install Script Options

The `install-deps.ps1` script supports several options:

```powershell
# Install only Rust
.\install-deps.ps1 -InstallRust

# Install only Perl
.\install-deps.ps1 -InstallPerl

# Install only Perl::Critic
.\install-deps.ps1 -InstallPerlCritic

# Install everything and run build
.\install-deps.ps1 -RunBuild

# Skip specific components
.\install-deps.ps1 -SkipRust -SkipPerl

# Install everything (default behavior)
.\install-deps.ps1
```

## Troubleshooting

### PowerShell Execution Policy

If you get an execution policy error, run:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

### PATH Issues

After installation, you may need to restart your terminal or IDE for PATH changes to take effect.

### Perl::Critic Not Found

If Perl::Critic is not found after installation:

1. Verify Strawberry Perl is installed: `perl --version`
2. Check if Perl::Critic is installed: `perl -MPerl::Critic -e "print 'OK'"`
3. If not installed, run: `C:\Strawberry\perl\bin\cpan.bat Perl::Critic`

## Verification

After installation, verify everything works:

```bash
# Check Rust
rustc --version
cargo --version

# Check Perl
perl --version

# Check Perl::Critic
perl -MPerl::Critic -e "print Perl::Critic->VERSION"

# Build the project
cargo build

# Test with Perl::Critic
cargo run fail 000 --perl-critic
```

## Requirements

- Windows 10 or later
- Administrator privileges (for Perl and Perl::Critic installation)
- Internet connection (for downloading installers)

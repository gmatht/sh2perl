#!/bin/bash
# Quick test script to verify Docker environment is set up correctly

set -e

echo "=== Testing Docker Environment ==="
echo ""

echo "1. Checking Rust installation..."
rustc --version
cargo --version
echo "✓ Rust is installed"
echo ""

echo "2. Checking Perl installation..."
perl --version | head -3
echo "✓ Perl is installed"
echo ""

echo "3. Checking Perl modules..."
perl -MPPI -e "print 'PPI: ' . PPI->VERSION . \"\n\"" || echo "✗ PPI not found"
perl -MPPI::Find -e "print 'PPI::Find: OK\n'" || echo "✗ PPI::Find not found"
perl -MPerl::Critic -e "print 'Perl::Critic: ' . Perl::Critic->VERSION . \"\n\"" || echo "✗ Perl::Critic not found"
perl -MGetopt::Long -e "print 'Getopt::Long: OK\n'" || echo "✗ Getopt::Long not found"
perl -MFile::Basename -e "print 'File::Basename: OK\n'" || echo "✗ File::Basename not found"
perl -MTime::HiRes -e "print 'Time::HiRes: OK\n'" || echo "✗ Time::HiRes not found"
perl -MPOSIX -e "print 'POSIX: OK\n'" || echo "✗ POSIX not found"
echo ""

echo "4. Checking project build..."
if [ -f "target/debug/debashc" ] || [ -f "target/debug/debashc.exe" ]; then
    echo "✓ Project is built"
else
    echo "Building project..."
    cargo build --bin debashc
    echo "✓ Project built successfully"
fi
echo ""

echo "5. Testing purify.pl --help..."
perl purify.pl --help > /dev/null 2>&1 && echo "✓ purify.pl --help works" || echo "✗ purify.pl --help failed"
echo ""

echo "6. Testing debashc..."
if [ -f "target/debug/debashc" ]; then
    ./target/debug/debashc --help > /dev/null 2>&1 && echo "✓ debashc works" || echo "✗ debashc failed"
elif [ -f "target/debug/debashc.exe" ]; then
    ./target/debug/debashc.exe --help > /dev/null 2>&1 && echo "✓ debashc works" || echo "✗ debashc failed"
else
    echo "✗ debashc binary not found"
fi
echo ""

echo "=== Docker Environment Test Complete ==="
echo ""
echo "To run tests: bash ./fail"
echo "To run specific test: bash ./fail TEST_PREFIX"





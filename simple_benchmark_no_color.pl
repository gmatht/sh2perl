#!/usr/bin/env perl

use strict;
use warnings;
use Time::HiRes qw(time);
use POSIX qw(strftime);

# Simple benchmark script for sh2perl comparison (no color version)
# This version works with minimal dependencies and no color output

my $ITERATIONS = 3;
my $WARMUP_RUNS = 1;

# Test cases to benchmark
my @TEST_CASES = (
    '001_simple',
    '002_control_flow',
    '003_pipeline',
    '004_test_quoted',
    '005_args',
    '006_misc',
    '007_cat_EOF',
    '008_simple_backup',
    '009_arrays',
    '044_find_example',
    '051_primes',
    '052_numeric_computations'
);

sub log_message {
    my ($message) = @_;
    my $timestamp = strftime("%H:%M:%S", localtime);
    print "[$timestamp] $message\n";
}

sub show_progress_bar {
    my ($current, $total, $width) = @_;
    $width ||= 50;
    
    my $percentage = $current / $total;
    my $filled = int($percentage * $width);
    my $empty = $width - $filled;
    
    my $bar = "[" . ("█" x $filled) . ("░" x $empty) . "]";
    my $percent_str = sprintf("%.1f%%", $percentage * 100);
    
    print "\r$bar $percent_str ($current/$total)";
    print "\n" if $current == $total;
}

sub show_interim_result {
    my ($test_name, $shell_time, $perl_time, $iteration, $total_iterations) = @_;
    
    my $speedup = $shell_time > 0 && $perl_time > 0 ? $shell_time / $perl_time : 0;
    my $status = $speedup > 1 ? "faster" : "slower";
    
    print "  Iteration $iteration/$total_iterations: ";
    printf "Shell: %.4fs, Perl: %.4fs, %.2fx %s\n", 
           $shell_time, $perl_time, $speedup, $status;
}

sub clear_line {
    print "\r" . (" " x 80) . "\r";
}

sub run_command {
    my ($command) = @_;
    
    my $start_time = time();
    my $output = `$command 2>&1`;
    my $end_time = time();
    my $exit_code = $? >> 8;
    
    return {
        elapsed => $end_time - $start_time,
        output => $output,
        exit_code => $exit_code,
        success => $exit_code == 0
    };
}

sub benchmark_test {
    my ($test_name, $test_index, $total_tests) = @_;
    
    my $shell_script = "examples/$test_name.sh";
    my $perl_script = "examples.pl/$test_name.pl";
    
    # Check if files exist
    unless (-f $shell_script) {
        log_message("WARNING: Shell script not found: $shell_script");
        return undef;
    }
    
    unless (-f $perl_script) {
        log_message("WARNING: Perl script not found: $perl_script");
        return undef;
    }
    
    print "\n";
    log_message("Benchmarking $test_name...");
    
    my @shell_times = ();
    my @perl_times = ();
    my $shell_output = "";
    my $perl_output = "";
    
    # Show overall progress
    show_progress_bar($test_index, $total_tests);
    
    # Warmup runs
    print "  Warming up...";
    for (1..$WARMUP_RUNS) {
        run_command("bash $shell_script");
        run_command("perl $perl_script");
    }
    clear_line();
    
    # Actual benchmark runs
    for (1..$ITERATIONS) {
        print "  Running iteration $_/$ITERATIONS...";
        
        # Test shell script
        my $shell_result = run_command("bash $shell_script");
        if ($shell_result->{success}) {
            push @shell_times, $shell_result->{elapsed};
            $shell_output = $shell_result->{output} if $_ == 1;
        } else {
            log_message("  WARNING: Shell script failed with exit code $shell_result->{exit_code}");
        }
        
        # Test Perl script
        my $perl_result = run_command("perl $perl_script");
        if ($perl_result->{success}) {
            push @perl_times, $perl_result->{elapsed};
            $perl_output = $perl_result->{output} if $_ == 1;
        } else {
            log_message("  WARNING: Perl script failed with exit code $perl_result->{exit_code}");
        }
        
        # Show interim result
        if (@shell_times > 0 && @perl_times > 0) {
            clear_line();
            show_interim_result($test_name, $shell_times[-1], $perl_times[-1], $_, $ITERATIONS);
        } else {
            clear_line();
        }
    }
    
    # Show final result for this test
    if (@shell_times > 0 && @perl_times > 0) {
        my $shell_avg = calculate_average(\@shell_times);
        my $perl_avg = calculate_average(\@perl_times);
        my $speedup = $shell_avg / $perl_avg;
        my $status = $speedup > 1 ? "faster" : "slower";
        
        print "  Final: ";
        printf "Shell: %.4fs, Perl: %.4fs, %.2fx %s\n", 
               $shell_avg, $perl_avg, $speedup, $status;
    }
    
    return {
        test_name => $test_name,
        shell_times => \@shell_times,
        perl_times => \@perl_times,
        shell_output => $shell_output,
        perl_output => $perl_output
    };
}

sub calculate_average {
    my ($times) = @_;
    return 0 unless @$times;
    my $sum = 0;
    $sum += $_ for @$times;
    return $sum / @$times;
}

sub generate_report {
    my ($results) = @_;
    
    print "\n" . "="x80 . "\n";
    print "SH2PERL BENCHMARK RESULTS\n";
    print "="x80 . "\n\n";
    
    # Summary table
    print "SUMMARY TABLE\n";
    print "-"x80 . "\n";
    printf "%-20s %-12s %-12s %-12s %-12s\n", 
           "Test Name", "Shell (s)", "Perl (s)", "Speedup", "Status";
    print "-"x80 . "\n";
    
    my $total_shell_time = 0;
    my $total_perl_time = 0;
    my $test_count = 0;
    
    for my $result (@$results) {
        next unless $result;
        
        my $shell_avg = calculate_average($result->{shell_times});
        my $perl_avg = calculate_average($result->{perl_times});
        
        next unless $shell_avg > 0 && $perl_avg > 0;
        
        my $speedup = $shell_avg / $perl_avg;
        my $status = "OK";
        
        # Check if outputs are similar (basic comparison)
        if ($result->{shell_output} ne $result->{perl_output}) {
            $status = "DIFF";
        }
        
        printf "%-20s %-12.4f %-12.4f %-12.2fx %-12s\n",
               $result->{test_name}, $shell_avg, $perl_avg, $speedup, $status;
        
        $total_shell_time += $shell_avg;
        $total_perl_time += $perl_avg;
        $test_count++;
    }
    
    print "-"x80 . "\n";
    if ($test_count > 0) {
        my $overall_speedup = $total_shell_time / $total_perl_time;
        printf "%-20s %-12.4f %-12.4f %-12.2fx %-12s\n",
               "OVERALL AVERAGE", $total_shell_time/$test_count, $total_perl_time/$test_count, 
               $overall_speedup, "OK";
    }
    print "="x80 . "\n\n";
    
    # Detailed results
    print "DETAILED RESULTS\n";
    print "="x80 . "\n\n";
    
    for my $result (@$results) {
        next unless $result;
        
        print "Test: $result->{test_name}\n";
        print "-"x50 . "\n";
        
        my $shell_avg = calculate_average($result->{shell_times});
        my $perl_avg = calculate_average($result->{perl_times});
        
        if ($shell_avg > 0) {
            print "Shell Script Performance:\n";
            printf "  Average: %.4f seconds\n", $shell_avg;
            printf "  Times: %s\n", join(", ", map { sprintf("%.4f", $_) } @{$result->{shell_times}});
        }
        
        if ($perl_avg > 0) {
            print "Perl Script Performance:\n";
            printf "  Average: %.4f seconds\n", $perl_avg;
            printf "  Times: %s\n", join(", ", map { sprintf("%.4f", $_) } @{$result->{perl_times}});
        }
        
        if ($shell_avg > 0 && $perl_avg > 0) {
            my $speedup = $shell_avg / $perl_avg;
            print "Performance Comparison:\n";
            printf "  Perl is %.2fx %s than shell\n", 
                   $speedup, $speedup > 1 ? "faster" : "slower";
        }
        
        # Show output differences if any
        if ($result->{shell_output} ne $result->{perl_output}) {
            print "Output Differences Detected:\n";
            print "  Shell output length: " . length($result->{shell_output}) . " chars\n";
            print "  Perl output length: " . length($result->{perl_output}) . " chars\n";
        }
        
        print "\n";
    }
}

# Main execution
print "="x80 . "\n";
print "SH2PERL BENCHMARK SYSTEM\n";
print "="x80 . "\n\n";

log_message("Starting sh2perl benchmark...");
log_message("Iterations per test: $ITERATIONS");
log_message("Warmup runs: $WARMUP_RUNS");

my @results = ();

# Use command line arguments if provided, otherwise use default test cases
my @tests_to_run = @ARGV ? @ARGV : @TEST_CASES;

print "\nRunning " . scalar(@tests_to_run) . " test(s)...\n";

for my $i (0..$#tests_to_run) {
    my $test_name = $tests_to_run[$i];
    my $result = benchmark_test($test_name, $i + 1, scalar(@tests_to_run));
    push @results, $result if $result;
}

generate_report(\@results);

log_message("Benchmark completed.");

1;


#!/usr/bin/env perl

use strict;
use warnings;
use Time::HiRes qw(time);
use Term::ANSIColor qw(colored);
use POSIX qw(strftime);

# Simple benchmark script for sh2perl comparison
# This version works with minimal dependencies

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
    print colored("[$timestamp] ", "blue") . "$message\n";
}

sub show_progress_bar {
    my ($current, $total, $width) = @_;
    $width ||= 50;
    
    my $percentage = $current / $total;
    my $filled = int($percentage * $width);
    my $empty = $width - $filled;
    
    my $bar = "[" . ("█" x $filled) . ("░" x $empty) . "]";
    my $percent_str = sprintf("%.1f%%", $percentage * 100);
    
    print "\r" . colored($bar, "green") . " " . colored($percent_str, "yellow") . " ($current/$total)";
    print "\n" if $current == $total;
}

sub show_interim_result {
    my ($test_name, $shell_time, $perl_time, $iteration, $total_iterations) = @_;
    
    my $speedup = $shell_time > 0 && $perl_time > 0 ? $shell_time / $perl_time : 0;
    my $status = $speedup > 1 ? "faster" : "slower";
    my $color = $speedup > 1 ? "green" : "red";
    
    print colored("  Iteration $iteration/$total_iterations: ", "cyan");
    print colored(sprintf("Shell: %.4fs, Perl: %.4fs, %.2fx %s", 
                         $shell_time, $perl_time, $speedup, $status), $color) . "\n";
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
        log_message(colored("WARNING: Shell script not found: $shell_script", "yellow"));
        return undef;
    }
    
    unless (-f $perl_script) {
        log_message(colored("WARNING: Perl script not found: $perl_script", "yellow"));
        return undef;
    }
    
    print "\n";
    log_message(colored("Benchmarking $test_name...", "bold white"));
    
    my @shell_times = ();
    my @perl_times = ();
    my $shell_output = "";
    my $perl_output = "";
    
    # Show overall progress
    show_progress_bar($test_index, $total_tests);
    
    # Warmup runs
    print colored("  Warming up...", "dim");
    for (1..$WARMUP_RUNS) {
        run_command("bash $shell_script");
        run_command("perl $perl_script");
    }
    clear_line();
    
    # Actual benchmark runs
    for (1..$ITERATIONS) {
        print colored("  Running iteration $_/$ITERATIONS...", "cyan");
        
        # Test shell script
        my $shell_result = run_command("bash $shell_script");
        if ($shell_result->{success}) {
            push @shell_times, $shell_result->{elapsed};
            $shell_output = $shell_result->{output} if $_ == 1;
        } else {
            log_message(colored("  WARNING: Shell script failed with exit code $shell_result->{exit_code}", "red"));
        }
        
        # Test Perl script
        my $perl_result = run_command("perl $perl_script");
        if ($perl_result->{success}) {
            push @perl_times, $perl_result->{elapsed};
            $perl_output = $perl_result->{output} if $_ == 1;
        } else {
            log_message(colored("  WARNING: Perl script failed with exit code $perl_result->{exit_code}", "red"));
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
        my $color = $speedup > 1 ? "green" : "red";
        
        print colored("  Final: ", "bold");
        print colored(sprintf("Shell: %.4fs, Perl: %.4fs, %.2fx %s", 
                             $shell_avg, $perl_avg, $speedup, $status), $color) . "\n";
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
    
    print "\n" . colored("="x80, "bold blue") . "\n";
    print colored("SH2PERL BENCHMARK RESULTS", "bold white") . "\n";
    print colored("="x80, "bold blue") . "\n\n";
    
    # Summary table
    print colored("SUMMARY TABLE", "bold cyan") . "\n";
    print colored("-"x80, "blue") . "\n";
    printf colored("%-20s %-12s %-12s %-12s %-12s\n", "bold", 
           "Test Name", "Shell (s)", "Perl (s)", "Speedup", "Status");
    print colored("-"x80, "blue") . "\n";
    
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
        my $status_color = "green";
        
        # Check if outputs are similar (basic comparison)
        if ($result->{shell_output} ne $result->{perl_output}) {
            $status = "DIFF";
            $status_color = "yellow";
        }
        
        my $speedup_color = $speedup > 1 ? "green" : "red";
        
        printf colored("%-20s ", "white") . 
               colored("%-12.4f ", "cyan") . 
               colored("%-12.4f ", "magenta") . 
               colored("%-12.2fx ", $speedup_color) . 
               colored("%-12s\n", $status_color),
               $result->{test_name}, $shell_avg, $perl_avg, $speedup, $status;
        
        $total_shell_time += $shell_avg;
        $total_perl_time += $perl_avg;
        $test_count++;
    }
    
    print colored("-"x80, "blue") . "\n";
    if ($test_count > 0) {
        my $overall_speedup = $total_shell_time / $total_perl_time;
        my $overall_color = $overall_speedup > 1 ? "green" : "red";
        printf colored("%-20s ", "bold white") . 
               colored("%-12.4f ", "bold cyan") . 
               colored("%-12.4f ", "bold magenta") . 
               colored("%-12.2fx ", "bold $overall_color") . 
               colored("%-12s\n", "bold green"),
               "OVERALL AVERAGE", $total_shell_time/$test_count, $total_perl_time/$test_count, 
               $overall_speedup, "OK";
    }
    print colored("="x80, "bold blue") . "\n\n";
    
    # Detailed results
    print colored("DETAILED RESULTS", "bold cyan") . "\n";
    print colored("="x80, "bold blue") . "\n\n";
    
    for my $result (@$results) {
        next unless $result;
        
        print colored("Test: $result->{test_name}", "bold white") . "\n";
        print colored("-"x50, "blue") . "\n";
        
        my $shell_avg = calculate_average($result->{shell_times});
        my $perl_avg = calculate_average($result->{perl_times});
        
        if ($shell_avg > 0) {
            print colored("Shell Script Performance:", "cyan") . "\n";
            printf colored("  Average: %.4f seconds\n", "white"), $shell_avg;
            printf colored("  Times: %s\n", "dim"), join(", ", map { sprintf("%.4f", $_) } @{$result->{shell_times}});
        }
        
        if ($perl_avg > 0) {
            print colored("Perl Script Performance:", "magenta") . "\n";
            printf colored("  Average: %.4f seconds\n", "white"), $perl_avg;
            printf colored("  Times: %s\n", "dim"), join(", ", map { sprintf("%.4f", $_) } @{$result->{perl_times}});
        }
        
        if ($shell_avg > 0 && $perl_avg > 0) {
            my $speedup = $shell_avg / $perl_avg;
            my $comparison_color = $speedup > 1 ? "green" : "red";
            print colored("Performance Comparison:", "bold") . "\n";
            printf colored("  Perl is %.2fx %s than shell\n", $comparison_color), 
                   $speedup, $speedup > 1 ? "faster" : "slower";
        }
        
        # Show output differences if any
        if ($result->{shell_output} ne $result->{perl_output}) {
            print colored("Output Differences Detected:", "yellow") . "\n";
            print colored("  Shell output length: " . length($result->{shell_output}) . " chars\n", "dim");
            print colored("  Perl output length: " . length($result->{perl_output}) . " chars\n", "dim");
        }
        
        print "\n";
    }
}

# Main execution
print colored("="x80, "bold blue") . "\n";
print colored("SH2PERL BENCHMARK SYSTEM", "bold white") . "\n";
print colored("="x80, "bold blue") . "\n\n";

log_message(colored("Starting sh2perl benchmark...", "bold green"));
log_message("Iterations per test: $ITERATIONS");
log_message("Warmup runs: $WARMUP_RUNS");

my @results = ();

# Use command line arguments if provided, otherwise use default test cases
my @tests_to_run = @ARGV ? @ARGV : @TEST_CASES;

print "\n" . colored("Running " . scalar(@tests_to_run) . " test(s)...", "bold cyan") . "\n";

for my $i (0..$#tests_to_run) {
    my $test_name = $tests_to_run[$i];
    my $result = benchmark_test($test_name, $i + 1, scalar(@tests_to_run));
    push @results, $result if $result;
}

generate_report(\@results);

log_message("Benchmark completed.");

1;

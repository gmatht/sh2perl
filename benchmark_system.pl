#!/usr/bin/env perl

use strict;
use warnings;
use Time::HiRes qw(gettimeofday tv_interval);
use File::Temp qw(tempfile);
use File::Spec;
use File::Basename;
use Cwd qw(abs_path getcwd);
use Data::Dumper;

# Benchmark configuration
my $BENCHMARK_CONFIG = {
    iterations => 5,           # Number of iterations per test
    warmup_runs => 2,         # Warmup runs to exclude from timing
    timeout => 30,            # Timeout in seconds per run
    memory_check => 1,        # Enable memory usage tracking
    verbose => 1,             # Verbose output
};

# Test categories and their corresponding examples
my $TEST_CATEGORIES = {
    'simple_operations' => [
        '001_simple',
        '002_control_flow', 
        '003_pipeline',
        '004_test_quoted',
        '005_args'
    ],
    'file_operations' => [
        '044_find_example',
        '007_cat_EOF',
        '008_simple_backup',
        '050_test_ls_star_dot_sh'
    ],
    'text_processing' => [
        '015_grep_advanced',
        '016_grep_basic',
        '017_grep_context',
        '018_grep_params',
        '019_grep_regex'
    ],
    'arrays_and_data' => [
        '009_arrays',
        '028_arrays_indexed',
        '029_arrays_associative'
    ],
    'complex_operations' => [
        '012_process_substitution',
        '013_parameter_expansion',
        '014_ansi_quoting',
        '058_advanced_bash_idioms'
    ],
    'mathematical' => [
        '051_primes',
        '052_numeric_computations',
        '053_gcd',
        '054_fibonacci',
        '055_factorize'
    ]
};

# Global results storage
my %benchmark_results = ();

sub log_message {
    my ($level, $message) = @_;
    my $timestamp = scalar localtime;
    print "[$timestamp] [$level] $message\n" if $BENCHMARK_CONFIG->{verbose};
}

sub create_test_environment {
    my $repo_root = dirname(abs_path($0));
    my $test_dir = File::Spec->catdir($repo_root, '.test-work', 'benchmark');
    require File::Path;
    File::Path::remove_tree($test_dir);
    File::Path::make_path($test_dir);
    log_message("INFO", "Created test environment: $test_dir");
    return $test_dir;
}

sub measure_execution_time {
    my ($command, $timeout) = @_;
    $timeout ||= $BENCHMARK_CONFIG->{timeout};
    
    my $start_time = [gettimeofday];
    
    # Execute command with timeout
    my $pid = open(my $pipe, "-|", "timeout $timeout $command 2>&1");
    if (!$pid) {
        log_message("ERROR", "Failed to execute command: $command");
        return { success => 0, error => "Failed to execute command" };
    }
    
    my $output = "";
    while (<$pipe>) {
        $output .= $_;
    }
    close($pipe);
    
    my $end_time = [gettimeofday];
    my $elapsed = tv_interval($start_time, $end_time);
    
    return {
        success => 1,
        elapsed_time => $elapsed,
        output => $output,
        exit_code => $? >> 8
    };
}

sub measure_memory_usage {
    my ($command) = @_;
    
    # Use /usr/bin/time if available (Linux/Unix)
    if (-x "/usr/bin/time") {
        my $result = `timeout $BENCHMARK_CONFIG->{timeout} /usr/bin/time -f "%M" $command 2>&1`;
        if ($result =~ /(\d+)$/) {
            return $1; # Peak memory usage in KB
        }
    }
    
    # Fallback: basic memory check using ps (less accurate)
    my $pid = open(my $pipe, "-|", "$command & echo \$!");
    if ($pid) {
        my $child_pid = <$pipe>;
        chomp $child_pid;
        close($pipe);
        
        sleep 0.1; # Brief pause to let process start
        
        my $memory = `ps -o rss= -p $child_pid 2>/dev/null`;
        chomp $memory;
        
        # Wait for process to complete
        waitpid($child_pid, 0);
        
        return $memory || 0;
    }
    
    return 0;
}

sub run_shell_script {
    my ($script_path, $test_dir) = @_;
    
    # Make script executable
    chmod 0755, $script_path;
    
    # Change to test directory and run
    my $original_dir = getcwd();
    chdir $test_dir;
    
    my $result = measure_execution_time("bash \"$script_path\"");
    
    chdir $original_dir;
    return $result;
}

sub run_perl_script {
    my ($script_path, $test_dir) = @_;
    
    # Change to test directory and run
    my $original_dir = getcwd();
    chdir $test_dir;
    
    my $result = measure_execution_time("perl \"$script_path\"");
    
    chdir $original_dir;
    return $result;
}

sub benchmark_single_test {
    my ($test_name, $test_dir) = @_;
    
    my $repo_root = dirname(abs_path($0));
    my $shell_script = File::Spec->catfile($repo_root, 'examples', "$test_name.sh");
    my $perl_script = File::Spec->catfile($repo_root, 'examples.pl', "$test_name.pl");
    
    # Check if both files exist
    unless (-f $shell_script && -f $perl_script) {
        log_message("WARN", "Missing files for test $test_name: $shell_script, $perl_script");
        return undef;
    }
    
    log_message("INFO", "Benchmarking $test_name...");
    
    my $results = {
        test_name => $test_name,
        shell_times => [],
        perl_times => [],
        shell_memory => [],
        perl_memory => [],
        shell_output => "",
        perl_output => ""
    };
    
    # Warmup runs
    for (1..$BENCHMARK_CONFIG->{warmup_runs}) {
        run_shell_script($shell_script, $test_dir);
        run_perl_script($perl_script, $test_dir);
    }
    
    # Actual benchmark runs
    for (1..$BENCHMARK_CONFIG->{iterations}) {
        log_message("INFO", "  Run $test_name iteration $_/$BENCHMARK_CONFIG->{iterations}");
        
        # Test shell script
        my $shell_result = run_shell_script($shell_script, $test_dir);
        if ($shell_result->{success}) {
            push @{$results->{shell_times}}, $shell_result->{elapsed_time};
            $results->{shell_output} = $shell_result->{output} if $_ == 1; # Store first run output
        }
        
        # Test Perl script
        my $perl_result = run_perl_script($perl_script, $test_dir);
        if ($perl_result->{success}) {
            push @{$results->{perl_times}}, $perl_result->{elapsed_time};
            $results->{perl_output} = $perl_result->{output} if $_ == 1; # Store first run output
        }
        
        # Memory measurement (if enabled)
        if ($BENCHMARK_CONFIG->{memory_check}) {
            my $shell_memory = measure_memory_usage("bash \"$shell_script\"");
            my $perl_memory = measure_memory_usage("perl \"$perl_script\"");
            push @{$results->{shell_memory}}, $shell_memory;
            push @{$results->{perl_memory}}, $perl_memory;
        }
    }
    
    return $results;
}

sub calculate_statistics {
    my ($values) = @_;
    return {} unless @$values;
    
    my $sum = 0;
    my $min = $values->[0];
    my $max = $values->[0];
    
    for my $val (@$values) {
        $sum += $val;
        $min = $val if $val < $min;
        $max = $val if $val > $max;
    }
    
    my $mean = $sum / @$values;
    
    # Calculate standard deviation
    my $variance = 0;
    for my $val (@$values) {
        $variance += ($val - $mean) ** 2;
    }
    $variance /= @$values;
    my $std_dev = sqrt($variance);
    
    return {
        mean => $mean,
        min => $min,
        max => $max,
        std_dev => $std_dev,
        count => scalar @$values
    };
}

sub generate_report {
    my ($results) = @_;
    
    print "\n" . "="x80 . "\n";
    print "SH2PERL BENCHMARK RESULTS\n";
    print "="x80 . "\n\n";
    
    # Summary table
    print "SUMMARY TABLE\n";
    print "-"x80 . "\n";
    printf "%-25s %-12s %-12s %-12s %-12s\n", 
           "Test Name", "Shell (s)", "Perl (s)", "Speedup", "Memory Ratio";
    print "-"x80 . "\n";
    
    my $total_shell_time = 0;
    my $total_perl_time = 0;
    my $test_count = 0;
    
    for my $test_name (sort keys %$results) {
        my $result = $results->{$test_name};
        next unless $result;
        
        my $shell_stats = calculate_statistics($result->{shell_times});
        my $perl_stats = calculate_statistics($result->{perl_times});
        
        next unless $shell_stats->{count} > 0 && $perl_stats->{count} > 0;
        
        my $speedup = $shell_stats->{mean} / $perl_stats->{mean};
        my $memory_ratio = 0;
        
        if ($BENCHMARK_CONFIG->{memory_check} && @{$result->{shell_memory}} > 0 && @{$result->{perl_memory}} > 0) {
            my $shell_mem_stats = calculate_statistics($result->{shell_memory});
            my $perl_mem_stats = calculate_statistics($result->{perl_memory});
            $memory_ratio = $perl_mem_stats->{mean} / $shell_mem_stats->{mean} if $shell_mem_stats->{mean} > 0;
        }
        
        printf "%-25s %-12.4f %-12.4f %-12.2fx %-12.2fx\n",
               $test_name, $shell_stats->{mean}, $perl_stats->{mean}, $speedup, $memory_ratio;
        
        $total_shell_time += $shell_stats->{mean};
        $total_perl_time += $perl_stats->{mean};
        $test_count++;
    }
    
    print "-"x80 . "\n";
    if ($test_count > 0) {
        my $overall_speedup = $total_shell_time / $total_perl_time;
        printf "%-25s %-12.4f %-12.4f %-12.2fx %-12s\n",
               "OVERALL AVERAGE", $total_shell_time/$test_count, $total_perl_time/$test_count, 
               $overall_speedup, "N/A";
    }
    print "="x80 . "\n\n";
    
    # Detailed results
    print "DETAILED RESULTS\n";
    print "="x80 . "\n\n";
    
    for my $test_name (sort keys %$results) {
        my $result = $results->{$test_name};
        next unless $result;
        
        print "Test: $test_name\n";
        print "-"x50 . "\n";
        
        my $shell_stats = calculate_statistics($result->{shell_times});
        my $perl_stats = calculate_statistics($result->{perl_times});
        
        if ($shell_stats->{count} > 0) {
            print "Shell Script Performance:\n";
            printf "  Mean: %.4f seconds\n", $shell_stats->{mean};
            printf "  Min:  %.4f seconds\n", $shell_stats->{min};
            printf "  Max:  %.4f seconds\n", $shell_stats->{max};
            printf "  Std:  %.4f seconds\n", $shell_stats->{std_dev};
        }
        
        if ($perl_stats->{count} > 0) {
            print "Perl Script Performance:\n";
            printf "  Mean: %.4f seconds\n", $perl_stats->{mean};
            printf "  Min:  %.4f seconds\n", $perl_stats->{min};
            printf "  Max:  %.4f seconds\n", $perl_stats->{max};
            printf "  Std:  %.4f seconds\n", $perl_stats->{std_dev};
        }
        
        if ($shell_stats->{count} > 0 && $perl_stats->{count} > 0) {
            my $speedup = $shell_stats->{mean} / $perl_stats->{mean};
            print "Performance Comparison:\n";
            printf "  Perl is %.2fx %s than shell\n", 
                   $speedup, $speedup > 1 ? "faster" : "slower";
        }
        
        if ($BENCHMARK_CONFIG->{memory_check} && @{$result->{shell_memory}} > 0 && @{$result->{perl_memory}} > 0) {
            my $shell_mem_stats = calculate_statistics($result->{shell_memory});
            my $perl_mem_stats = calculate_statistics($result->{perl_memory});
            print "Memory Usage:\n";
            printf "  Shell: %.0f KB (avg)\n", $shell_mem_stats->{mean};
            printf "  Perl:  %.0f KB (avg)\n", $perl_mem_stats->{mean};
        }
        
        print "\n";
    }
}

sub run_benchmark_suite {
    my ($categories) = @_;
    
    log_message("INFO", "Starting benchmark suite...");
    log_message("INFO", "Configuration: " . Dumper($BENCHMARK_CONFIG));
    
    my $test_dir = create_test_environment();
    my $repo_root = dirname(abs_path($0));
    
    my %all_results = ();
    
    for my $category (keys %$categories) {
        log_message("INFO", "Running category: $category");
        
        for my $test_name (@{$categories->{$category}}) {
            my $result = benchmark_single_test($test_name, $test_dir);
            if ($result) {
                $all_results{$test_name} = $result;
            }
        }
    }
    
    generate_report(\%all_results);
    
    # Save results to file
    my $results_file = File::Spec->catfile($repo_root, '.test-work', 'benchmark_results.json');
    open(my $fh, '>', $results_file) or die "Cannot write results file: $!";
    print $fh JSON::encode_json(\%all_results);
    close($fh);
    
    log_message("INFO", "Results saved to: $results_file");
    log_message("INFO", "Benchmark suite completed.");
}

# Main execution
if (@ARGV) {
    # Run specific tests
    my @test_names = @ARGV;
    my %specific_tests = ();
    for my $test_name (@test_names) {
        $specific_tests{$test_name} = [$test_name];
    }
    run_benchmark_suite(\%specific_tests);
} else {
    # Run all tests
    run_benchmark_suite($TEST_CATEGORIES);
}

1;

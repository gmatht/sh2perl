#!/usr/bin/env perl
use strict;
use warnings;
use Getopt::Long;
use File::Basename;
use File::Temp qw(tempfile);
use File::Path qw(make_path remove_tree);
use File::Spec;
use Cwd qw(abs_path getcwd);
use Time::HiRes qw(time);
use POSIX qw(WIFEXITED WEXITSTATUS);

# Command line options
my $verbose = 1;
my $next = 0;
my $purify_tested=0;
my $purify_passed=0;
my $purify_failed=0;

# Fine-grained timeout settings
my %timeouts = (
    'purify_help' => 5,        # purify.pl --help should be very fast
    'purify_execution' => 30,  # purify.pl execution - reduced from 60s
    'grep_check' => 5,         # grep operations should be fast
    'perl_execution' => 15,    # Perl script execution - reduced from 30s
    'diff_comparison' => 10,   # diff operations
    'file_operations' => 10,   # file I/O operations
);

# Debug levels
my $debug_level = 2;  # 0=none, 1=basic, 2=detailed, 3=verbose

GetOptions(
	'verbose|v' => \$verbose,
    'next' => \$next,
) or die "Error in command line arguments\n";

if ($verbose) {
    print "Running test_purify.pl with verbose output\n";
}

if ($next) {
    print "Running test_purify.pl with next option\n";
}

$ENV{TZ} = 'UTC';
$ENV{LANG} = 'C';
$ENV{LC_ALL} = 'C';
$ENV{PATH} = '/usr/bin:/bin';
umask 022;

my $repo_root = dirname(abs_path($0));
my $workspace_root = File::Spec->catdir($repo_root, '.test-work', 'purify');
make_path($workspace_root);
my $purify_pl = File::Spec->catfile($repo_root, 'purify.pl');
my $debashc_path = File::Spec->catfile($repo_root, 'target', 'debug', 'debashc');
# On Windows try the .exe suffix if the plain name isn't executable
if ($^O eq 'MSWin32' && !-x $debashc_path && -f "$debashc_path.exe") {
    $debashc_path .= '.exe';
}

# If the file doesn't exist, error out.
if (!-e $debashc_path) {
    die "Error: debashc not found at '$debashc_path'. Please build the project first with 'cargo build'\n";
}

# If the file exists but is not executable, attempt to set the executable
# bit on Unix-like systems so the test harness can run it. If chmod fails
# or we're on Windows and the file isn't executable, bail with an error.
if (!-x $debashc_path) {
    if ($^O ne 'MSWin32') {
        my $ok = chmod 0755, $debashc_path;
        if ($ok) {
            print "Notice: set executable bit on '$debashc_path'\n" if $verbose;
        } else {
            die "Error: debashc exists at '$debashc_path' but is not executable and chmod failed. Please run 'chmod +x $debashc_path' or rebuild with cargo.\n";
        }
    } else {
        die "Error: debashc not found or not executable at '$debashc_path'. Please build the project first with 'cargo build'\n";
    }
}

# Enhanced debugging functions
sub debug_print {
    my ($level, $message) = @_;
    return if $level > $debug_level;
    print "DEBUG: $message\n";
}

sub debug_progress {
    my ($current, $total, $operation) = @_;
    my $percent = int(($current / $total) * 100);
    my $bar_length = 20;
    my $filled = int(($current / $total) * $bar_length);
    my $bar = "[" . "=" x $filled . " " x ($bar_length - $filled) . "]";
    debug_print(1, "Progress: $bar $percent% ($current/$total) - $operation");
}

sub print_output_excerpt {
    my ($title, $output, $max_lines) = @_;
    $max_lines ||= 8;
    $output //= '';

    $output =~ s/\r\n/\n/g;
    $output =~ s/\r/\n/g;

    my @lines = split /\n/, $output;
    my $total_lines = scalar @lines;
    my $shown_lines = $total_lines < $max_lines ? $total_lines : $max_lines;

    print "$title (showing first $shown_lines of $total_lines lines)\n";
    if ($shown_lines == 0) {
        print "  <no output>\n";
        return;
    }

    for my $i (0 .. $shown_lines - 1) {
        print $lines[$i] . "\n";
    }

    print "... truncated after $shown_lines lines ...\n" if $total_lines > $shown_lines;
}

# Enhanced timeout functions with fine-grained control
sub run_system_with_timeout {
    my ($command, $timeout_type, $description) = @_;
    my $timeout = $timeouts{$timeout_type} || $timeouts{'file_operations'};
    $description ||= "system command";
    
    debug_print(2, "Starting $description (timeout: ${timeout}s, type: $timeout_type)");
    debug_print(3, "Command: $command");
    my $start_time = time();
    
    my $result;
    
    if ($^O eq 'MSWin32') {
        # Use PowerShell timeout on Windows
        my $ps_command = "powershell.exe -ExecutionPolicy Bypass -File \"ps_timeout.ps1\" -TimeoutSeconds $timeout -Command \"$command\" -Description \"$description\" 2>&1";
        $result = system($ps_command);
        
        # Check if it was a timeout (exit code 124)
        if ($result == 124) {
            debug_print(1, "TIMEOUT after ${timeout}s for $description");
            return -1;
        }
    } else {
        # Use alarm for timeout on Unix systems
        local $SIG{ALRM} = sub { 
            debug_print(1, "TIMEOUT after ${timeout}s for $description");
            die "Timeout after ${timeout}s for $description\n";
        };
        
        eval {
            alarm($timeout);
            $result = system($command);
            alarm(0);
        };
        
        if ($@) {
            alarm(0);
            my $duration = time() - $start_time;
            debug_print(1, "$description failed after ${duration}s: $@");
            return -1;
        }
    }
    
    my $end_time = time();
    my $duration = $end_time - $start_time;
    
    my $exit_code = $result >> 8;
    debug_print(2, "$description completed in ${duration}s (exit code: $exit_code)");
    if ($duration > $timeout * 0.8) {
        debug_print(1, "WARNING: $description took ${duration}s (${timeout}s timeout) - may need longer timeout");
    }
    return $exit_code;
}

sub run_backticks_with_timeout {
    my ($command, $timeout_type, $description) = @_;
    my $timeout = $timeouts{$timeout_type} || $timeouts{'file_operations'};
    $description ||= "backticks command";
    
    debug_print(2, "Starting $description (timeout: ${timeout}s, type: $timeout_type)");
    debug_print(3, "Command: $command");
    my $start_time = time();
    
    # Windows-compatible timeout using PowerShell
    my $result;
    my $exit_code;
    
    if ($^O eq 'MSWin32') {
        # Use PowerShell timeout on Windows
        my $ps_command = "powershell.exe -ExecutionPolicy Bypass -File \"ps_timeout.ps1\" -TimeoutSeconds $timeout -Command \"$command\" -Description \"$description\" 2>&1";
        $result = `$ps_command`;
        $exit_code = $? >> 8;
        
        # Check if it was a timeout (exit code 124)
        if ($exit_code == 124) {
            debug_print(1, "TIMEOUT after ${timeout}s for $description");
            return (undef, -1);
        }
    } else {
        # Use alarm for timeout on Unix systems
        local $SIG{ALRM} = sub { 
            debug_print(1, "TIMEOUT after ${timeout}s for $description");
            die "Timeout after ${timeout}s for $description\n";
        };
        
        eval {
            alarm($timeout);
            $result = `$command`;
            alarm(0);
        };
        
        if ($@) {
            alarm(0);
            my $duration = time() - $start_time;
            debug_print(1, "$description failed after ${duration}s: $@");
            return (undef, -1);
        }
        $exit_code = $? >> 8;
    }
    
    my $end_time = time();
    my $duration = $end_time - $start_time;
    
    debug_print(2, "$description completed in ${duration}s (exit code: $exit_code)");
    if ($duration > $timeout * 0.8) {
        debug_print(1, "WARNING: $description took ${duration}s (${timeout}s timeout) - may need longer timeout");
    }
    return ($result, $exit_code);
}

# Test purify.pl on all files from examples.impurl
my @test_files = glob(File::Spec->catfile($repo_root, 'examples.impurl', '*.pl'));
# Limit to first 10 files for faster testing
#@test_files = @test_files[0..9] if scalar(@test_files) > 10;
my $total_files = scalar(@test_files);

debug_print(1, "Found $total_files .pl files in examples.impurl directory");
debug_print(2, "Files to process: " . join(", ", @test_files));

my $tested_count = 0;
my $skipped_count = 0;
my $start_time = time();

# Test that purify.pl can handle the --help option
debug_print(1, "Testing purify.pl --help...");
my $perl_cmd = $^O eq 'MSWin32' ? "C:\\Strawberry\\perl\\bin\\perl.exe" : "perl";
my ($help_output, $help_result) = run_backticks_with_timeout("$perl_cmd \"$purify_pl\" --help 2>&1", 'purify_help', "purify.pl help test");
if ($help_result != 0) {
    debug_print(1, "Error: purify.pl --help failed (exit code: $help_result)");
    debug_print(1, "Error output: $help_output");
    die "Stopping on first failure. Fix the issue and run again.\n";
}
debug_print(1, "purify.pl --help test passed");

sub assert_rewrites_backticks {
    my ($name, $source, $expected_regex) = @_;

    my ($in_fh, $input_path) = tempfile();
    print $in_fh $source;
    close $in_fh;

    my ($out_fh, $output_path) = tempfile();
    close $out_fh;

    my $command = "$perl_cmd \"$purify_pl\" --debashc-path \"$debashc_path\" \"$input_path\" > \"$output_path\" 2>&1";
    my ($output, $result) = run_backticks_with_timeout($command, 'purify_execution', $name);
    if ($result != 0) {
        die "$name failed: $output\n";
    }

    my $purified = do {
        local $/;
        open my $fh, '<', $output_path or die "Cannot open $output_path: $!";
        <$fh>;
    };

    if ($purified =~ /`/) {
        die "$name still contains raw backticks:\n$purified\n";
    }

    if ($purified !~ $expected_regex) {
        die "$name did not rewrite to expected Perl:\n$purified\n";
    }

    unlink $input_path, $output_path;
}

assert_rewrites_backticks(
    'backtick command substitution',
    "print `echo hi`\n",
    qr/\('hi'\)\s*\.\s*"\\n"/,
);

assert_rewrites_backticks(
    'bare mv backtick',
    "`mv a b`\n",
    qr/\bmove\(/,
);

# Remove old comparison artifacts so they don't affect ls-based examples.
unlink 'out1.txt', 'out2.txt';


foreach my $perl_file (@test_files) {
    if (-f $perl_file) {
        $purify_tested++;
        my $example_name = basename($perl_file, '.pl');
        my $example_dir = File::Spec->catdir($workspace_root, $example_name);
        remove_tree($example_dir);
        make_path(File::Spec->catdir($example_dir, 'pure'));
        my $pure_file = File::Spec->catfile($example_dir, 'pure', basename($perl_file));

        # Show progress
        debug_progress($purify_tested, $total_files, "Processing files");
        debug_print(1, "Testing purify.pl on $perl_file -> $pure_file");
        debug_print(2, "File $purify_tested of $total_files");

        my $original_dir = getcwd();
        chdir $example_dir or die "Cannot chdir to $example_dir: $!\n";
        my $nondeterministic_skip = 0;

        eval {
            # Test purify.pl on the Perl file and capture output
            debug_print(2, "Running purify.pl on $perl_file");
            my ($output, $purify_result) = run_backticks_with_timeout("$perl_cmd \"$purify_pl\" --debashc-path \"$debashc_path\" \"$perl_file\" > \"$pure_file\" 2>&1", 'purify_execution', "purify.pl execution");
            debug_print(2, "purify.pl result: $purify_result");

            if ($purify_result == 0) {
                debug_print(1, "✓ $perl_file: purify.pl processed successfully");

                # Check if purified file still contains system calls or backticks
                debug_print(2, "Checking if $pure_file still contains system calls or backticks");
                # Use Perl to check for system calls and backticks (works on all platforms)
                my $check_script = qq{$perl_cmd -ne "if (/system|\\`/) { exit 1; }" "$pure_file"};
                my $grep_result = run_system_with_timeout($check_script, 'grep_check', "grep check");
                if ( $grep_result != 0 ){
                    debug_print(1, "Failed to Purify $pure_file - still contains system calls or backticks");
                    die "Failed to Purify $pure_file - still contains system calls or backticks\n";
                }
                debug_print(2, "✓ Purification check passed - no system calls or backticks found");

                # Run original file
                debug_print(2, "Running original file: $perl_file");
                my ($out1_fh, $out1_path) = tempfile(DIR => $workspace_root);
                close $out1_fh;
                my ($out1, $perl1_result) = run_backticks_with_timeout("$perl_cmd \"$perl_file\" > \"$out1_path\" 2>&1", 'perl_execution', "original file execution");
                debug_print(2, "Original file execution result: $perl1_result");

                # Run purified file
                debug_print(2, "Running purified file: $pure_file");
                my ($out2_fh, $out2_path) = tempfile(DIR => $workspace_root);
                close $out2_fh;
                my ($out2, $perl2_result) = run_backticks_with_timeout("$perl_cmd \"$pure_file\" > \"$out2_path\" 2>&1", 'perl_execution', "purified file execution");
                debug_print(2, "Purified file execution result: $perl2_result");

                # Compare outputs by reading the actual files
                my $file1_content = do {
                    local $/;
                    open my $fh, '<', $out1_path or die "Cannot open $out1_path: $!";
                    <$fh>;
                };
                my $file2_content = do {
                    local $/;
                    open my $fh, '<', $out2_path or die "Cannot open $out2_path: $!";
                    <$fh>;
                };

                if ( $file1_content ne $file2_content ) {
                    debug_print(1, "Output mismatch detected between original and purified files; re-running original to check for nondeterminism");
                    debug_print(2, " === purified === \n$output\n === end purified ===");

                    my ($out1b_fh, $out1b_path) = tempfile(DIR => $workspace_root);
                    close $out1b_fh;
                    my ($out1b, $perl1b_result) = run_backticks_with_timeout("$perl_cmd \"$perl_file\" > \"$out1b_path\" 2>&1", 'perl_execution', "original file re-run");
                    my $file1b_content = do {
                        local $/;
                        open my $fh, '<', $out1b_path or die "Cannot open $out1b_path: $!";
                        <$fh>;
                    };

                    if ($perl1_result != $perl1b_result || $file1_content ne $file1b_content) {
                        debug_print(1, "Nondeterministic test detected for $perl_file; skipping");
                        $skipped_count++;
                        $nondeterministic_skip = 1;
                        unlink $out1_path, $out2_path, $out1b_path;
                    } else {
                        my $diff_command;
                        if ($^O eq 'MSWin32') {
                            # Use fc on Windows
                            $diff_command = "fc \"$out1_path\" \"$out2_path\" 2>&1";
                        } else {
                            # Use diff on Unix systems
                            $diff_command = "diff -u \"$out1_path\" \"$out2_path\" 2>&1";
                        }
                        my ($diff_output, $diff_result) = run_backticks_with_timeout($diff_command, 'diff_comparison', "diff comparison");
                        print_output_excerpt("Diff excerpt", $diff_output, 8);
                        die "FAILED - Output mismatch for $perl_file -> $pure_file (diff exit code: $diff_result)\n";
                    }
                }
                if ($nondeterministic_skip) {
                    debug_print(1, "Skipping $perl_file after nondeterminism check");
                } else {
                    debug_print(1, "✓ Output comparison passed - files produce identical output");
                    unlink $out1_path, $out2_path;
                    $purify_passed++;
                }
            } else {
                debug_print(1, "✗ $perl_file: purify.pl failed (exit code: $purify_result)");
                debug_print(1, "Error output: $output");
                print_output_excerpt("purify.pl error output", $output, 8);
                $purify_failed++;
                # Quit on first failure
                die "Stopping on first failure. Fix the issue and run again.\n";
            }
        };

        my $example_error = $@;
        chdir $original_dir or die "Cannot chdir back to $original_dir: $!\n";
        next if $example_error eq '' && $nondeterministic_skip;
        die $example_error if $example_error;
    }
}

# Final summary with timing information
my $end_time = time();
my $total_duration = $end_time - $start_time;
my $avg_time_per_file = $total_duration / $purify_tested if $purify_tested > 0;

debug_print(1, "=== TEST SUMMARY ===");
debug_print(1, "Purify.pl test summary: $purify_passed passed, $purify_failed failed out of $purify_tested tested");
debug_print(1, "Total execution time: ${total_duration}s");
debug_print(1, "Average time per file: ${avg_time_per_file}s") if $purify_tested > 0;
debug_print(1, "Files processed: $purify_tested of $total_files");

# If any purify tests failed, this is a critical error
if ($purify_failed > 0) {
    debug_print(1, "Error: $purify_failed purify.pl tests failed. The purify.pl script is not working correctly.");
    die "Error: $purify_failed purify.pl tests failed. The purify.pl script is not working correctly.\n";
}

debug_print(1, "✓ test_purify.pl completed successfully");
debug_print(1, "Summary: Tested $tested_count files, skipped $skipped_count files");
debug_print(1, "Purify.pl tests: $purify_passed passed, $purify_failed failed out of $purify_tested tested");

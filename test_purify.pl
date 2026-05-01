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
# By default keep output minimal — only failures and concise pass lines.
my $verbose = 0;
my $next = 0;
my $purify_tested=0;
my $purify_passed=0;
my $purify_failed=0;
my $fatal_error = '';
my @test_failures = ();  # collect all failures so tests continue past the first

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
# Keep debug output off by default so only failures (with explicit prints)
# and concise PASSED lines are shown. Use the script internals to raise
# debug if needed during development.
my $debug_level = 0;  # 0=none, 1=basic, 2=detailed, 3=verbose

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

sub print_final_summary {
    my ($passed, $matching, $tested, $skipped, $failed, $start_time) = @_;
    my $end_time = time();
    my $total_duration = $end_time - $start_time;
    my $avg_time_per_file = $tested > 0 ? $total_duration / $tested : 0;

    debug_print(1, "=== TEST SUMMARY ===");
    debug_print(1, "Purify.pl test summary: $passed passed, $failed failed out of $tested tested");
    debug_print(1, "Total execution time: ${total_duration}s");
    debug_print(1, "Average time per file: ${avg_time_per_file}s") if $tested > 0;
    debug_print(1, "Files processed: $tested of $tested");
    debug_print(1, "test_purify.pl completed successfully");
    debug_print(1, "Summary: Tested $tested files, skipped $skipped files");
    print "Purify.pl tests: $passed passed +$matching lines match, $failed failed out of $tested tested\n";
    print "PROGRESS " . $passed . ":" . $matching . "\n";
}

# Write an authoritative failure report used by main_loop.pl
sub write_failure_report {
    my ($perl_file, $pure_file, $reason, $extra_text) = @_;
    $reason ||= 'unknown';
    $extra_text ||= '';
    my $failure_file = File::Spec->catfile($workspace_root, 'failure_report.txt');
    if (open my $fh, '>', $failure_file) {
        print $fh "failed_test: $perl_file\n";
        print $fh "pure_file: $pure_file\n";
        print $fh "reason: $reason\n\n";

        print $fh $extra_text;
        close $fh;
        debug_print(1, "Wrote failure report to $failure_file");
    } else {
        debug_print(1, "Failed to write failure report to $failure_file: $!");
    }
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
my $first_lines_match_in_failing_test = 0;

# Test that purify.pl can handle the --help option
debug_print(1, "Testing purify.pl --help...");
my $perl_cmd = $^O eq 'MSWin32' ? "C:\\Strawberry\\perl\\bin\\perl.exe" : "perl";
my ($help_output, $help_result) = run_backticks_with_timeout("$perl_cmd \"$purify_pl\" --help 2>&1", 'purify_help', "purify.pl help test");
if ($help_result != 0) {
    debug_print(1, "Error: purify.pl --help failed (exit code: $help_result)");
    debug_print(1, "Error output: $help_output");
    $fatal_error = "Stopping on first failure. Fix the issue and run again.\n";
    last;
}
print "PASSED: purify_help\n";

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
    # concise pass output for this assertion
    print "PASSED: $name\n";
}

sub count_matching_leading_lines {
    my ($left, $right) = @_;
    $left  //= '';
    $right //= '';

    $left =~ s/\r\n/\n/g;
    $left =~ s/\r/\n/g;
    $right =~ s/\r\n/\n/g;
    $right =~ s/\r/\n/g;

    my @left_lines = split /\n/, $left, -1;
    my @right_lines = split /\n/, $right, -1;
    my $limit = @left_lines < @right_lines ? scalar(@left_lines) : scalar(@right_lines);
    my $count = 0;
    for my $i (0 .. $limit - 1) {
        last if $left_lines[$i] ne $right_lines[$i];
        $count++;
    }
    return $count;
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


TEST_FILE:
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
                debug_print(1, "$perl_file: purify.pl processed successfully");

                # Check if purified file still contains system calls or backticks
                debug_print(2, "Checking if $pure_file still contains system calls or backticks");
                # Use a small Perl script written to a temp file to detect
                # untranslated system(...) and backticks. Writing a file
                # avoids complex escaping issues when embedding the script
                # as a one-liner.
                my ($check_fh, $check_path) = tempfile(DIR => $workspace_root, SUFFIX => '.pl');
                print $check_fh <<'PERL_SCRIPT';
use strict;
use warnings;
my $f = shift or exit 1;
open my $fh, '<', $f or exit 1;
local $/;
my $s = <$fh>;
close $fh;
my $failed = 0;
while ($s =~ /system\s*\(\s*(["'])(.*?)\1\s*\)/g) {
    my $cmdline = $2;
    my ($cmd) = split(/\s+/, $cmdline);
    next unless $cmd;
    my $check_cmd = $^O eq 'MSWin32' ? "where $cmd >NUL 2>&1" : "command -v $cmd >/dev/null 2>&1";
    my $rc = system($check_cmd);
    # If the command exists on PATH then it's an untranslated system() call -> fail
    if ($rc == 0) { $failed = 1; last; }
}
if ($s =~ /`/) { $failed = 1; }
exit($failed ? 1 : 0);
PERL_SCRIPT
                close $check_fh;
                my $grep_result = run_system_with_timeout("$perl_cmd \"$check_path\" \"$pure_file\"", 'grep_check', "grep check");
                unlink $check_path;
                if ( $grep_result != 0 ){
                    debug_print(1, "Failed to Purify $pure_file - still contains system calls or backticks");
                    die "Failed to Purify $pure_file - still contains system calls or backticks\n";
                }
                debug_print(2, "Purification check passed - no system calls or backticks found");

                # Run original file — capture stdout and stderr to separate files
                debug_print(2, "Running original file: $perl_file");
                my ($out1_stdout_fh, $out1_stdout) = tempfile(DIR => $workspace_root);
                close $out1_stdout_fh;
                my ($out1_stderr_fh, $out1_stderr) = tempfile(DIR => $workspace_root);
                close $out1_stderr_fh;
                my ($out1, $perl1_result) = run_backticks_with_timeout(
                    "$perl_cmd \"$perl_file\" > \"$out1_stdout\" 2> \"$out1_stderr\"",
                    'perl_execution', "original file execution");
                debug_print(2, "Original file execution result: $perl1_result");

                # Run purified file — separate stdout and stderr
                debug_print(2, "Running purified file: $pure_file");
                my ($out2_stdout_fh, $out2_stdout) = tempfile(DIR => $workspace_root);
                close $out2_stdout_fh;
                my ($out2_stderr_fh, $out2_stderr) = tempfile(DIR => $workspace_root);
                close $out2_stderr_fh;
                my ($out2, $perl2_result) = run_backticks_with_timeout(
                    "$perl_cmd \"$pure_file\" > \"$out2_stdout\" 2> \"$out2_stderr\"",
                    'perl_execution', "purified file execution");
                debug_print(2, "Purified file execution result: $perl2_result");

                # Helper to slurp a file
                my $slurp = sub {
                    my ($path) = @_;
                    local $/;
                    open my $fh, '<', $path or die "Cannot open $path: $!";
                    scalar <$fh>;
                };

                my $file1_stdout = $slurp->($out1_stdout);
                my $file1_stderr = $slurp->($out1_stderr);
                my $file2_stdout = $slurp->($out2_stdout);
                my $file2_stderr = $slurp->($out2_stderr);

                # Normalize Perl runtime warnings that include the source file
                # path and line number (e.g. "at /abs/path/to/file.pl line 42.")
                # before comparing, since the original and purified scripts live
                # in different directories and will naturally produce different
                # paths/line numbers in such messages even when the behaviour is
                # semantically identical.
                my $normalize_perl_warnings = sub {
                    my ($s) = @_;
                    # Normalize "at FILE line N." (FILE may contain spaces on some OSes)
                    $s =~ s{ at .+? line \d+\.}{ at <source> line N.}mg;
                    return $s;
                };
                my $norm1_stderr = $normalize_perl_warnings->($file1_stderr);
                my $norm2_stderr = $normalize_perl_warnings->($file2_stderr);

                my $stdout_match = ($file1_stdout eq $file2_stdout);
                my $stderr_match = ($norm1_stderr eq $norm2_stderr);

                if ( !$stdout_match || !$stderr_match ) {
                    debug_print(1, "Output mismatch detected between original and purified files; re-running original to check for nondeterminism");
                    debug_print(2, " === purified === \n$output\n === end purified ===");
                    $first_lines_match_in_failing_test = count_matching_leading_lines($file1_stdout, $file2_stdout);

                    # Re-run original to detect nondeterminism
                    my ($out1b_stdout_fh, $out1b_stdout) = tempfile(DIR => $workspace_root);
                    close $out1b_stdout_fh;
                    my ($out1b_stderr_fh, $out1b_stderr) = tempfile(DIR => $workspace_root);
                    close $out1b_stderr_fh;
                    my ($out1b, $perl1b_result) = run_backticks_with_timeout(
                        "$perl_cmd \"$perl_file\" > \"$out1b_stdout\" 2> \"$out1b_stderr\"",
                        'perl_execution', "original file re-run");
                    my $file1b_stdout = $slurp->($out1b_stdout);
                    my $file1b_stderr = $slurp->($out1b_stderr);

                    if ($perl1_result != $perl1b_result
                            || $file1_stdout ne $file1b_stdout
                            || $file1_stderr ne $file1b_stderr) {
                        debug_print(1, "Nondeterministic test detected for $perl_file; skipping");
                        $skipped_count++;
                        $nondeterministic_skip = 1;
                        unlink $out1_stdout, $out1_stderr, $out2_stdout, $out2_stderr, $out1b_stdout, $out1b_stderr;
                        # Remove any prior failure report for this workspace since the test is nondeterministic
                        my $maybe_failure = File::Spec->catfile($workspace_root, 'failure_report.txt');
                        unlink $maybe_failure if -e $maybe_failure;
                    } else {
                        # Build combined diff for reporting (stdout diff + stderr diff)
                        my $diff_output = '';
                        if (!$stdout_match) {
                            if ($^O eq 'MSWin32') {
                                my ($d, undef) = run_backticks_with_timeout("fc \"$out1_stdout\" \"$out2_stdout\" 2>&1", 'diff_comparison', "stdout diff");
                                $diff_output .= "=== stdout diff ===\n$d";
                            } else {
                                my ($d, undef) = run_backticks_with_timeout("diff -u \"$out1_stdout\" \"$out2_stdout\" 2>&1", 'diff_comparison', "stdout diff");
                                $diff_output .= "=== stdout diff ===\n$d";
                            }
                        }
                        if (!$stderr_match) {
                            if ($^O eq 'MSWin32') {
                                my ($d, undef) = run_backticks_with_timeout("fc \"$out1_stderr\" \"$out2_stderr\" 2>&1", 'diff_comparison', "stderr diff");
                                $diff_output .= "=== stderr diff ===\n$d";
                            } else {
                                my ($d, undef) = run_backticks_with_timeout("diff -u \"$out1_stderr\" \"$out2_stderr\" 2>&1", 'diff_comparison', "stderr diff");
                                $diff_output .= "=== stderr diff ===\n$d";
                            }
                        }
                        # Store the failing test and the diff into the workspace for main_loop.pl to consume
                        write_failure_report($perl_file, $pure_file, 'output_mismatch', $diff_output);
                        print "FAILED: $example_name\n";
                        print "Full diff for $perl_file -> $pure_file:\n$diff_output\n";
                        push @test_failures, "FAILED - Output mismatch for $perl_file -> $pure_file";
                        $purify_failed++;
                        die "NEXT_TEST\n";
                    }
                }
                if ($nondeterministic_skip) {
                    debug_print(1, "Skipping $perl_file after nondeterminism check");
                } else {
                    unlink $out1_stdout, $out1_stderr, $out2_stdout, $out2_stderr;
                    $purify_passed++;
                    # Only print concise pass line for successful tests
                    print "PASSED: $example_name\n";
                }
            } else {
                debug_print(1, "✗ $perl_file: purify.pl failed (exit code: $purify_result)");
                debug_print(1, "Error output: $output");
                print "FAILED: $example_name\n";
                print "purify.pl error output for $perl_file:\n$output\n";
                # Record the purify.pl failure output for main_loop.pl
                write_failure_report($perl_file, $pure_file, 'purify_execution_failed', $output);
                $first_lines_match_in_failing_test = 0;
                $purify_failed++;
                push @test_failures, "FAILED - purify.pl failed for $perl_file";
                die "NEXT_TEST\n";
            }
        };

        my $example_error = $@;
        chdir $original_dir or die "Cannot chdir back to $original_dir: $!\n";
        next if $example_error eq "NEXT_TEST\n";  # test failed, already recorded above
        next if $example_error eq '' && $nondeterministic_skip;
        if ($example_error) {
            $purify_failed++;
            push @test_failures, "ERROR in $perl_file: $example_error";
            print "ERROR: $example_name ($example_error)";
            next;
        }
    }
}

print_final_summary($purify_passed, $first_lines_match_in_failing_test, $purify_tested, $skipped_count, $purify_failed, $start_time);

if (@test_failures) {
    print "\n=== FAILED TESTS (" . scalar(@test_failures) . ") ===\n";
    print "$_\n" for @test_failures;
    die scalar(@test_failures) . " test(s) failed\n";
}

if ($fatal_error ne '') {
    die $fatal_error;
}

exit 0;

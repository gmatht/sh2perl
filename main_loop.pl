#!/usr/bin/env perl
use strict;
use warnings;
use Time::HiRes qw(sleep);
use File::Spec;

$| = 1; 
print "Auto flush enabled\n";

our $exit_code;

sub run_purify() {
my $test_cmd = $^O eq 'MSWin32' ? 'perl test_purify.pl' : './test_purify.pl';
open(my $pipe, '-|', 'perl', './test_purify.pl') or die "Cannot run test_purify.pl: $!";
open(my $out, '>', 'purify.out') or die "Cannot open purify.out: $!";
while (my $line = <$pipe>) {
    print $line;
    print $out $line;
}
close($out);
close($pipe);
    $exit_code = $? >> 8;
print "Ran\n";
open(my $fh, '<', 'purify.out') or die "Cannot open purify.out: $!";
my $output = do { local $/; <$fh> };
close($fh);
print "Slurped\n";

my $length = 10000;
my $start = length($output) - $length;
$start = 0 if $start < 0; # Prevent negative start if string is too short
my $last_10k = substr($output, $start);
    # Prefer the focused failure report if it exists
    my $failure_file = File::Spec->catfile('.test-work', 'purify', 'failure_report.txt');
    if (-e $failure_file) {
        if (open my $ff, '<', $failure_file) {
            local $/;
            my $report = <$ff>;
            close $ff;
            return $report;
        }
    }
    return $last_10k;
}

while (1) {
	#print "Running $test_cmd \n";
    #my $output = `$test_cmd 2>&1`;
    #Work In Progress
    #system("stdbuf -o0 perl ./test_purify.pl 2>&1 | tee > purify.out");

    print "\nInvoking opencode to fix the failure...\n";

    my $output=run_purify();

    my $prompt = join("\n",
        "Fix the failure reported by test_purify.pl.",
        "Use the output below as the task description and make the smallest correct code change.",
       "Try to fix the underlying Rust code, keeping purify.pl a thin wrapper around the real smarts in Rust, unless the bug is really in purify.pl",
       "read FIX.md, and after you have fixed the bug add a note to FIX.md as to what you fixed and why.",
        "",
        $output,
        "",
        "After fixing the issue, stop.",
    );

    # This CLI exposes prompt text via `opencode run --prompt`; top-level `-p` is password.
    #system('opencode', 'run', '--prompt', $prompt);
    #system('opencode', 'run',  '-m', 'github-copilot/gpt-5.4-mini', '--variant', 'xhigh', $prompt);
    #system('opencode', 'run',  '-m', 'github-copilot/gpt-5.4-mini', '--variant', 'high', $prompt);
    system('opencode', 'run',  '-m', 'github-copilot/gpt-5-mini', '--variant', 'high', $prompt);

    sleep 8;

#print "Ran $test_cmd \n";
#my $output = `cat purify.out`;
#print "Slurped $test_cmd \n";

    if ($exit_code == 0) {
        print $output;
        print "\nAll errors are fixed.\n";
        system('perl', './main_loop_rust.pl');
        last;
    }

    print $output;

    # Try to extract number of passed tests from the test output
    my $passed = 0;
    if ($output =~ /Purify\.pl(?: test summary| tests):\s*(\d+)\s+passed/s) {
        $passed = $1;
    } elsif ($output =~ /(\d+)\s+passed,?\s*\d+\s+failed/s) {
        $passed = $1;
    } else {
        # Fallback: test_purify.pl prints concise "PASSED: name" lines by
        # default rather than the debug summary. Count those lines so we
        # still detect the number of passing tests when the summary isn't
        # emitted.
        my $count = () = $output =~ /^PASSED:/mg;
        $passed = $count if $count > 0;
    }

    # Load previous max tests + matching lines from the summary line printed by test_purify.pl
    my $max_file = '.max_tests_passed';
    my $old_max = 0;
    my $old_matching = 0;
    if (-e $max_file) {
        if (open my $mf, '<', $max_file) {
            my $txt = <$mf>;
            close $mf;
            chomp $txt if defined $txt;
            if ($txt =~ /^(\d+):(\d+)$/) {
                $old_max = $1;
                $old_matching = $2;
            } elsif ($txt =~ /^(\d+)$/) {
                $old_max = $1;
                $old_matching = 0;
            }
        }
    }

    sub read_summary_metrics_from_output {
        my ($text) = @_;
        my ($passed_tests, $matching_lines) = (0, 0);
        if (defined $text) {
            my @lines = split /\n/, $text;
            for (my $i = $#lines; $i >= 0; $i--) {
                my $line = $lines[$i];
                next unless defined $line && $line =~ /\S/;
                if ($line =~ /^PROGRESS (\d+):(\d+)$/) {
                    ($passed_tests, $matching_lines) = ($1 + 0, $2 + 0);
                }
                last;
            }
        }
        return ($passed_tests, $matching_lines);
    }

    my ($summary_passed, $summary_matching) = read_summary_metrics_from_output($output);
    $passed = $summary_passed if $summary_passed > 0;

    if ($passed > $old_max) {
        # More tests passed: record and commit
        my $new_matching = $summary_matching;
        if (open my $mf, '>', $max_file) {
            print $mf "$passed:$new_matching";
            close $mf;
            system('git', 'add', $max_file);
        }
        my $msg = "More tests pass (${old_max}->${passed})";
        $msg .= " and matching lines (${old_matching}->${new_matching})" if $new_matching > 0;
        print "\nDetected improvement: $msg\n";
        system('git', 'commit', '.', '-m', $msg);
    } elsif ($passed == $old_max) {
        # Same number of passed tests — prefer greater number of matching first lines
        my $new_matching = $summary_matching;
        if ($new_matching > $old_matching) {
            if (open my $mf, '>', $max_file) {
                print $mf "$passed:$new_matching";
                close $mf;
                system('git', 'add', $max_file);
            }
            my $msg = "More matching stdout lines with same tests (${old_matching}->${new_matching})";
            print "\nDetected improvement: $msg\n";
            system('git', 'commit', '.', '-m', $msg);
        } else {
            # No improvement (equal or regression). Ask opencode whether to keep or stash.
            my $prompt = "No new tests pass, should the git diff in progress be accepted into the main branch. The final line of your answer should contain 'KEEP' or 'STASH'";
            $prompt .= "\n\nTest output:\n" . $output . "\n";

            print "\nInvoking opencode to ask whether to keep or stash changes...\n";

            my $oc_out = '';
            if (open my $oc, '-|', 'opencode', 'run',  '-m', 'github-copilot/gpt-5-mini', '--variant', 'high', $prompt) {
                local $/;
                $oc_out = <$oc>;
                close $oc;
            } else {
                warn "Could not run opencode: $!\n";
            }

            print "opencode response:\n" . ($oc_out // '') . "\n";

            # Determine final non-empty line from opencode output
            my $decision = 'DEBUG';
            if (defined $oc_out && $oc_out ne '') {
                my @lines = split /\n/, $oc_out;
                for (my $i = $#lines; $i >= 0; $i--) {
                    my $ln = $lines[$i];
                    next unless defined $ln && $ln =~ /\S/;
                    if ($ln =~ /KEEP/i) { $decision = 'KEEP'; last; }
                    if ($ln =~ /STASH/i) { $decision = 'STASH'; last; }
                }
            }

            if ($decision eq 'KEEP') {
                print "Keeping changes (committing)...\n";
                my $msg = "WIP accepted (tests: ${passed})";
                system('git', 'commit', '.', '-m', $msg);
            } else {
                if ($decision eq 'STASH') {
                    print "Stashing changes...\n";
                    system('git', 'stash', 'push', '-m', "auto-stash: tests ${old_max}->${passed}");
                } else {
                    print "No Decision made! ($decision)";
                }
            }
        }
    } else {
        # Regression: fewer tests passed than before.
        my $prompt = "The git diff results in a regression of tests passing. Is this the result of important refactoring that is worth keeping in and building on? In the final line of your answer say KEEP or STASH";
        $prompt .= "\n\nTest output:\n" . $output . "\n";

        print "\nInvoking opencode to ask whether to keep or stash changes...\n";

        my $oc_out = '';
        if (open my $oc, '-|', 'opencode', 'run',  '-m', 'github-copilot/gpt-5-mini', '--variant', 'high', $prompt) {
            local $/;
            $oc_out = <$oc>;
            close $oc;
        } else {
            warn "Could not run opencode: $!\n";
        }

        print "opencode response:\n" . ($oc_out // '') . "\n";

        my $decision = 'DEBUG';
        if (defined $oc_out && $oc_out ne '') {
            my @lines = split /\n/, $oc_out;
            for (my $i = $#lines; $i >= 0; $i--) {
                my $ln = $lines[$i];
                next unless defined $ln && $ln =~ /\S/;
                if ($ln =~ /KEEP/i) { $decision = 'KEEP'; last; }
                if ($ln =~ /STASH/i) { $decision = 'STASH'; last; }
            }
        }

        if ($decision eq 'KEEP') {
            print "Keeping changes (committing)...\n";
            my $msg = "Keep changes (tests: ${old_max}->${passed})";
            system('git', 'commit', '.', '-m', $msg);
        } elsif ($decision eq 'STASH') {
            print "Stashing changes...\n";
            system('git', 'stash', 'push', '-m', "auto-stash: tests ${old_max}->${passed}");
        } else {
            print "No Decision made! ($decision)";
        }
    }

}

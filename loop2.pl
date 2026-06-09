#!/usr/bin/env perl
use strict;
use warnings;
use Time::HiRes qw(sleep);

my $test_cmd = './fail';
my $MAX_RETRIES_PER_FAILURE = 10;

my $prev_failed = -1;
my $prev_passed = -1;

sub extract_failures {
    my ($output) = @_;
    my @sections = split /={80}\n\s+TEST FAILED\n={80}\n/, $output;
    shift @sections;
    my @failures;
    for my $section (@sections) {
        if ($section =~ /^File:\s+examples\/(\S+\.sh)/m) {
            my $file = $1;
            my $prefix = $1;
            $prefix =~ s/\.sh$//;
            my ($reason) = $section =~ /Failure Reason:\s*(.+)$/m;
            $reason //= 'unknown';
            push @failures, { file => $file, prefix => $prefix, section => $section, reason => $reason };
        }
    }
    return @failures;
}

sub parse_summary {
    my ($output) = @_;
    if ($output =~ /TESTS COMPLETED:\s*(\d+)\s+passed,\s*(\d+)\s+failed\s+out\s+of\s+(\d+)/) {
        return ($1 + 0, $2 + 0, $3 + 0);
    }
    if ($output =~ /ALL TESTS PASSED/) {
        if ($output =~ /Total tests:\s*(\d+)/) {
            my $total = $1 + 0;
            return ($total, 0, $total);
        }
    }
    return (undef, undef, undef);
}

sub keep_or_revert {
    my ($new_passed, $new_failed) = @_;
    if ($prev_failed < 0) { return 'keep' }

    if ($new_failed < $prev_failed) {
        print "\nPROGRESS: failures $prev_failed -> $new_failed (passed $prev_passed -> $new_passed). Keeping changes.\n";
        system('git', 'add', '-A');
        system('git', 'commit', '--allow-empty', '-m', "Progress: $new_passed passed, $new_failed failed (was $prev_passed / $prev_failed)");
        return 'keep';
    }

    if ($new_failed == $prev_failed) {
        print "\nSTABLE: failures unchanged at $new_failed (passed $new_passed). Keeping changes.\n";
        return 'keep';
    }

    print "\nREGRESSION: failures $prev_failed -> $new_failed (passed $prev_passed -> $new_passed). Reverting changes.\n";
    system('git', 'checkout', '--', '.');
    return 'revert';
}

while (1) {
    print "\n=== Running full test suite ===\n";
    my $output = `$test_cmd 2>&1`;
    my $exit_code = $?;

    my ($passed, $failed, $total) = parse_summary($output);
    $passed //= 0;
    $failed //= $exit_code == 0 ? 0 : scalar(() = $output =~ /TEST FAILED/g);

    print "\nSummary: $passed passed, $failed failed" . (defined $total ? " out of $total" : "") . "\n";

    if ($exit_code == 0 && !defined $total) {
        if ($output =~ /FAILED:|FAILURE/) {
            print "BUG: Failed did not raise exit code\n";
        } else {
            print $output;
            print "\nAll errors are fixed.\n";
            last;
        }
    }

    if ($failed == 0) {
        print $output;
        print "\nAll errors are fixed.\n";
        last;
    }

    my @failures = extract_failures($output);

    if (!@failures) {
        print $output;
        print "\nCould not parse individual failures. Invoking opencode with full output...\n";
        my $prompt = join("\n",
            "Fix the failure reported by fail.",
            "Use the output below as the task description and make the smallest correct code change.",
            "",
            $output,
            "",
            "After fixing the issue, stop.",
        );
        system('opencode', 'run', '-m', 'opencode-go/deepseek-v4-flash', '--variant', 'xhigh', $prompt);
        sleep 1;
        next;
    }

    print "\nFound " . scalar(@failures) . " failure(s). Walking through each one...\n";

    for my $failure (@failures) {
        my $prefix = $failure->{prefix};
        print "\n--- Fixing failure: $prefix.sh ($failure->{reason}) ---\n";

        my $retries = 0;
        while ($retries < $MAX_RETRIES_PER_FAILURE) {
            my $prompt = join("\n",
                "Fix the failure in test '$prefix.sh'.",
                "Use the output below as the task description and make the smallest correct code change.",
                "",
                $failure->{section},
                "",
                "After fixing the issue, stop.",
            );

            system('opencode', 'run', '-m', 'opencode-go/deepseek-v4-flash', '--variant', 'xhigh', $prompt);
            sleep 1;

            my $single_output = `$test_cmd $prefix 2>&1`;
            my $single_exit = $?;

            if ($single_exit == 0) {
                print "\nTest $prefix.sh now passes!\n";
                last;
            }

            print $single_output;
            print "\nFix attempt " . ($retries + 1) . " for $prefix.sh did not resolve, retrying...\n";

            $failure->{section} = $single_output;
            $retries++;
        }

        if ($retries >= $MAX_RETRIES_PER_FAILURE) {
            print "\nGiving up on $prefix.sh after $MAX_RETRIES_PER_FAILURE attempts.\n";
        }
    }

    print "\nAll identified failures processed. Re-running full test suite to measure progress...\n";
    my $new_output = `$test_cmd 2>&1`;
    my ($new_passed, $new_failed, $new_total) = parse_summary($new_output);
    $new_passed //= 0;
    $new_failed //= scalar(() = $new_output =~ /TEST FAILED/g);

    print "\nResults: $new_passed passed, $new_failed failed" . (defined $new_total ? " out of $new_total" : "") . "\n";

    keep_or_revert($new_passed, $new_failed);

    if ($new_failed == 0) {
        print $new_output;
        print "\nAll errors are fixed.\n";
        last;
    }

    ($prev_passed, $prev_failed) = ($new_passed, $new_failed);
}

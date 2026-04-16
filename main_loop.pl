#!/usr/bin/env perl
use strict;
use warnings;
use Time::HiRes qw(sleep);

$| = 1; 
print "Auto flush enabled\n";

my $test_cmd = $^O eq 'MSWin32' ? 'perl test_purify.pl' : './test_purify.pl';

while (1) {
    print "Running $test_cmd \n";
    #my $output = `$test_cmd 2>&1`;
    #Work In Progress
    #system("stdbuf -o0 perl ./test_purify.pl 2>&1 | tee > purify.out");

open(my $pipe, '-|', 'perl', './test_purify.pl') or die "Cannot run test_purify.pl: $!";
open(my $out, '>', 'purify.out') or die "Cannot open purify.out: $!";
while (my $line = <$pipe>) {
    print $line;
    print $out $line;
}
close($out);
close($pipe);
    my $exit_code = $? >> 8;
print "Ran\n";
open(my $fh, '<', 'purify.out') or die "Cannot open purify.out: $!";
my $output = do { local $/; <$fh> };
close($fh);
print "Slurped\n";

#print "Ran $test_cmd \n";
#my $output = `cat purify.out`;
#print "Slurped $test_cmd \n";

    if ($exit_code == 0) {
        print $output;
        print "\nAll errors are fixed.\n";
        last;
    }

    print $output;

    # Try to extract number of passed tests from the test output
    my $passed = 0;
    if ($output =~ /Purify\.pl(?: test summary| tests):\s*(\d+)\s+passed/s) {
        $passed = $1;
    } elsif ($output =~ /(\d+)\s+passed,?\s*\d+\s+failed/s) {
        $passed = $1;
    }

    # File that records the previous maximum passed tests
    my $max_file = '.max_tests_passed';
    my $old_max = 0;
    if (-e $max_file) {
        if (open my $mf, '<', $max_file) {
            my $txt = <$mf>;
            close $mf;
            chomp $txt if defined $txt;
            $old_max = $txt =~ /^(\d+)$/ ? $1 : 0;
        }
    }

    if ($passed > $old_max) {
        # Update the recorded max and commit the working tree
        if (open my $mf, '>', $max_file) {
            print $mf $passed;
            close $mf;
            system('git', 'add', $max_file);
        }
        my $msg = "More tests pass (${old_max}->${passed})";
        print "\nDetected improvement: $msg\n";
        system('git', 'commit', '.', '-m', $msg);
    } else {
        # No improvement (equal or regression). Ask opencode whether to keep or stash.
        my $prompt;
        if ($passed == $old_max) {
            $prompt = "No new tests pass, should this work in progress be accepted into the main branch. The final line of your answer should contain 'KEEP' or 'STASH'";
        } else {
            $prompt = "There is a regression in tests passing in this commit. Is this the result of important refactoring that is worth keeping in and building on? In the final line of your answer say KEEP or STASH";
        }

        # Include the failing output to provide context
        $prompt .= "\n\nTest output:\n" . $output . "\n";

        print "\nInvoking opencode to ask whether to keep or stash changes...\n";

        my $oc_out = '';
        if (open my $oc, '-|', 'opencode', 'run',  '-m', 'github-copilot/gpt-5-mini', '--variant', 'high', '--prompt', $prompt) {
            local $/;
            $oc_out = <$oc>;
            close $oc;
        } else {
            warn "Could not run opencode: $!\n";
        }

        print "opencode response:\n" . ($oc_out // '') . "\n";

        # Determine final non-empty line from opencode output
        my $decision = 'STASH';
        if (defined $oc_out && $oc_out ne '') {
            my @lines = split /\n/, $oc_out;
            for (my $i = $#lines; $i >= 0; $i--) {
                my $ln = $lines[$i];
                next unless defined $ln && $ln =~ /\S/;
                if ($ln =~ /KEEP/i) { $decision = 'KEEP'; last; }
                if ($ln =~ /STASH/i) { $decision = 'STASH'; last; }
                # If last non-empty line contains neither, fall through to default
                last;
            }
        }

        if ($decision eq 'KEEP') {
            print "Keeping changes (committing)...\n";
            my $msg = $passed == $old_max ? "WIP accepted (tests: ${passed})" : "Keep changes (tests: ${old_max}->${passed})";
            system('git', 'commit', '.', '-m', $msg);
        } else {
            print "Stashing changes...\n";
            system('git', 'stash', 'push', '-m', "auto-stash: tests ${old_max}->${passed}");
        }
    }
}

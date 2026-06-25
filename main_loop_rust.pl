#!/usr/bin/env perl
use strict;
use warnings;
use Time::HiRes qw(sleep);

my $test_cmd = './fail';

while (1) {
    my $output = `$test_cmd 2>&1`;
    my $exit_code = $?;

    if ($exit_code == 0) {
	if ($output=~/FAILED:|FAILURE/) {
		print ('BUG: Failed did not raise exit code');
	} else {
	        print $output;
        	print "\nAll errors are fixed.\n";
	        last;
	}
    }

    print $output;
    print "\nInvoking opencode to fix the failure...\n";

    my @lines = split("\n", $output);
    if (@lines > 50) {
        $output = join("\n", @lines[0..24]) . "\n...\n" . join("\n", @lines[-25..-1]);
    }

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
}

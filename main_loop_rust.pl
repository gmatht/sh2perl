#!/usr/bin/env perl
use strict;
use warnings;
use Time::HiRes qw(sleep);

#my $test_cmd = $^O eq 'MSWin32' ? 'test_purify.pl' : './test_purify.pl';
my $test_cmd = './fail';

while (1) {
    my $output = `$test_cmd 2>&1`;
    #my $exit_code = $? >> 8;
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

    my $prompt = join("\n",
        "Fix the failure reported by fail.",
        "Use the output below as the task description and make the smallest correct code change.",
        "",
        $output,
        "",
        "After fixing the issue, stop.",
    );

    # This CLI exposes prompt text via `opencode run --prompt`; top-level `-p` is password.
    #system('opencode', 'run', '--prompt', $prompt);
    system('opencode', 'run',  '-m', 'github-copilot/gpt-5.4-mini', '--variant', 'xhigh', $prompt);

    sleep 1;
}

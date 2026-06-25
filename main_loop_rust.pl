#!/usr/bin/env perl
use strict;
use warnings;
use File::Temp qw(tempfile);
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

    my $prompt = join("\n",
        "Fix the failure reported by fail.",
        "Use the output below as the task description and make the smallest correct code change.",
        "",
        $output,
        "",
        "After fixing the issue, stop.",
    );

    my ($fh, $filename) = tempfile(UNLINK => 1);
    print $fh $prompt;
    close $fh;

    system('opencode', 'run', '-m', 'opencode-go/deepseek-v4-flash', '--variant', 'xhigh', '--file', $filename);
    sleep 1;
}

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
    print "\nInvoking opencode to fix the failure...\n";

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
}

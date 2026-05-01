#!/usr/bin/perl

# Example 034: Demonstrate external nohup invocation (deterministic)

use strict;
use warnings;

print "=== Example 034: External nohup demonstration ===\n";

# Show invocation; avoid starting long-lived background processes in examples.
{
    my $c = "nohup sh -c 'echo nohup-demo' 2>/dev/null || true";
    print "\n$c\n";
    print qx/$c/;
}
{
    my $c = "nohup sh -c 'echo redirected' > nohup.out 2>&1 || true";
    print "\n$c\n";
    print qx/$c/;
}
{
    my $c = "cat nohup.out 2>/dev/null || echo '(no output file)'";
    print "\n$c\n";
    print qx/$c/;
}
unlink 'nohup.out' if -f 'nohup.out';

print "\nNote: This demonstrates safe, short nohup usage without persistent background jobs.\n";

print "\n=== Example 034 completed ===\n";

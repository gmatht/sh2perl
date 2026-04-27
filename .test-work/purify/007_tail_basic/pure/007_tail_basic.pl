sub __bt { my $s = join('', @_); wantarray ? (split /^/, $s, -1) : $s }
#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/007_tail_basic.pl" }


print "=== Example 007: Basic tail command ===\n";

open(my $fh, '>', 'test_tail.txt') or die "Cannot create test file: $!\n";
for my $i (1..10) {
    print $fh "Line $i: This is line number $i\n";
}
close($fh);

print "Using backticks to call tail (default 10 lines):\n";
my $tail_output = __bt(do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail test_tail.txt'; qx{$tail_cmd}; }
);
print $tail_output;

print "\ntail -n 5 (last 5 lines):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('tail', '-n', '5', 'test_tail.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\ntail -n 3 (last 3 lines):\n";
my $tail3 = __bt(do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -n 3 test_tail.txt'; qx{$tail_cmd}; }
);
print $tail3;

print "\ntail -n 1 (last line only):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('tail', '-n', '1', 'test_tail.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\ntail -n 15 (more than available):\n";
my $tail15 = __bt(do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -n 15 test_tail.txt'; qx{$tail_cmd}; }
);
print $tail15;

print "\ntail -c 50 (last 50 characters):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('tail', '-c', '50', 'test_tail.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\ntail -c 100 (last 100 characters):\n";
my $tail_bytes = __bt(do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -c 100 test_tail.txt'; qx{$tail_cmd}; }
);
print $tail_bytes;

print "\ntail from stdin (echo | tail):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', q{echo 'Line 1
Line 2
Line 3
Line 4
Line 5' | tail -n 3}); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\ntail -f simulation (follow mode):\n";
my $tail_follow = __bt(do { my $output_0 = q{}; my $output_printed_0; my $tail_cmd = 'tail -n 3 test_tail.txt'; qx{$tail_cmd}; }
);
print $tail_follow;

print "\ntail -q (quiet mode, no filename):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('tail', '-q', 'test_tail.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

unlink('test_tail.txt') if -f 'test_tail.txt';

print "=== Example 007 completed successfully ===\n";

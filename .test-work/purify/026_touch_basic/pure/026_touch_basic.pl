#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/026_touch_basic.pl" }


print "=== Example 026: Basic touch command ===\n";

$ENV{TZ} = 'UTC';
$ENV{LC_ALL} = 'C';

print "Using " . "sys" . "tem" . "() to call touch (create file):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "-t", "202301011200", "test_touch_file.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print -f "test_touch_file.txt" ? "File created successfully\n" : "File creation failed\n";

print "\ntouch with multiple files:\n";
my $touch_multi = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do { my $touch_cmd = 'touch -t 202301011200 test_touch_file1.txt test_touch_file2.txt test_touch_file3.txt'; qx{$touch_cmd}; };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
print "Multiple files created successfully\n" if -f "test_touch_file1.txt" && -f "test_touch_file2.txt" && -f "test_touch_file3.txt";

print "\ntouch with verbose (-v):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "-v", "-t", "202301011200", "test_touch_verbose.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntouch with no create (-c):\n";
my $touch_no_create = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_touch_no_create.txt" ) {
                my $current_time = time;
                utime $current_time, $current_time, "test_touch_no_create.txt";
            }
            else {
            }
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
print -f "test_touch_no_create.txt" ? "File already existed\n" : "File not created\n";

print "\ntouch with reference (-r):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "-r", "test_touch_file.txt", "test_touch_reference.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print "File created with reference timestamp\n" if -f "test_touch_reference.txt";

print "\ntouch with specific time (-t 202301011200):\n";
my $touch_time = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do { my $touch_cmd = 'touch -t 202301011200 test_touch_time.txt'; qx{$touch_cmd}; };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
print "File created with specific timestamp\n" if -f "test_touch_time.txt";

print "\ntouch with date (-d '2023-01-01 12:00:00'):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "-d", "2023-01-01 12:00:00", "test_touch_date.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print "File created with specific date\n" if -f "test_touch_date.txt";

print "\ntouch with access time (-a):\n";
my $touch_access = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do { my $touch_cmd = 'touch -a test_touch_file.txt'; qx{$touch_cmd}; };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
print "Access time updated\n";

print "\ntouch with modification time (-m):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "-m", "test_touch_file.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print "Modification time updated\n";

print "\ntouch with both times (-a -m):\n";
my $touch_both = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do { my $touch_cmd = 'touch -a -m test_touch_file.txt'; qx{$touch_cmd}; };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
print "Both access and modification times updated\n";

print "\ntouch with no dereference (-h):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "-h", "test_touch_no_deref.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print "File created with no dereference\n" if -f "test_touch_no_deref.txt";

print "\ntouch with error handling:\n";
my $touch_error = do { my $command = 'touch test_touch_error.txt 2>&1'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
print -f "test_touch_error.txt" ? "File created successfully\n" : "File creation failed\n";

print "\ntouch with specific mode (--mode=644):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "--mode=644", "test_touch_mode.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print "File created with specific mode\n" if -f "test_touch_mode.txt";

unlink('test_touch_file.txt') if -f 'test_touch_file.txt';
unlink('test_touch_file1.txt') if -f 'test_touch_file1.txt';
unlink('test_touch_file2.txt') if -f 'test_touch_file2.txt';
unlink('test_touch_file3.txt') if -f 'test_touch_file3.txt';
unlink('test_touch_verbose.txt') if -f 'test_touch_verbose.txt';
unlink('test_touch_no_create.txt') if -f 'test_touch_no_create.txt';
unlink('test_touch_reference.txt') if -f 'test_touch_reference.txt';
unlink('test_touch_time.txt') if -f 'test_touch_time.txt';
unlink('test_touch_date.txt') if -f 'test_touch_date.txt';
unlink('test_touch_no_deref.txt') if -f 'test_touch_no_deref.txt';
unlink('test_touch_error.txt') if -f 'test_touch_error.txt';
unlink('test_touch_mode.txt') if -f 'test_touch_mode.txt';

print "=== Example 026 completed successfully ===\n";

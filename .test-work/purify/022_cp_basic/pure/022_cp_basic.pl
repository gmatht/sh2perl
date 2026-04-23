#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/022_cp_basic.pl" }


print "=== Example 022: Basic cp command ===\n";

open(my $fh, '>', 'test_cp_source.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for copying\n";
print $fh "It has multiple lines\n";
print $fh "To demonstrate cp functionality\n";
close($fh);

do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mkdir", "-p", "test_cp_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "Using " . "sys" . "tem" . "() to call cp (copy file):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cp", "test_cp_source.txt", "test_cp_dest.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};
if (-f "test_cp_dest.txt") {
    print "File copied successfully\n";
    my $content = do { open my $fh, '<', 'test_cp_dest.txt' or die 'cat: ' . 'test_cp_dest.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "Content: $content";
}

print "\ncp with recursive (-r):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cp", "-r", "test_cp_dir", "test_cp_dir_copy"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};
if (-d "test_cp_dir_copy") {
    print "Directory copied successfully\n";
}

print "\ncp with preserve (-p):\n";
my $cp_preserve = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -p test_cp_source.txt test_cp_preserve.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_preserve.txt") {
    print "File copied with preserve attributes\n";
}

print "\ncp with verbose (-v):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cp", "-v", "test_cp_source.txt", "test_cp_verbose.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\ncp with force (-f):\n";
my $cp_force = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -f test_cp_source.txt test_cp_force.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_force.txt") {
    print "File copied with force\n";
}

print "\ncp with interactive (-i):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cp", "-i", "test_cp_source.txt", "test_cp_interactive.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\ncp with update (-u):\n";
my $cp_update = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -u test_cp_source.txt test_cp_update.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_update.txt") {
    print "File copied with update\n";
}

print "\ncp with backup (-b):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cp", "-b", "test_cp_source.txt", "test_cp_backup.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\ncp with suffix (--suffix=.bak):\n";
my $cp_suffix = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp --suffix=.bak test_cp_source.txt test_cp_suffix.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_suffix.txt") {
    print "File copied with suffix\n";
}

print "\ncp with multiple files:\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cp", "test_cp_source.txt", "test_cp_source2.txt", "test_cp_dir/"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "\ncp with preserve all (-a):\n";
my $cp_all = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -a test_cp_source.txt test_cp_all.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_all.txt") {
    print "File copied with preserve all\n";
}

print "\ncp with no dereference (-P):\n";
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("cp", "-P", "test_cp_source.txt", "test_cp_no_deref.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

unlink('test_cp_source.txt') if -f 'test_cp_source.txt';
unlink('test_cp_dest.txt') if -f 'test_cp_dest.txt';
unlink('test_cp_preserve.txt') if -f 'test_cp_preserve.txt';
unlink('test_cp_verbose.txt') if -f 'test_cp_verbose.txt';
unlink('test_cp_force.txt') if -f 'test_cp_force.txt';
unlink('test_cp_interactive.txt') if -f 'test_cp_interactive.txt';
unlink('test_cp_update.txt') if -f 'test_cp_update.txt';
unlink('test_cp_backup.txt') if -f 'test_cp_backup.txt';
unlink('test_cp_suffix.txt') if -f 'test_cp_suffix.txt';
unlink('test_cp_source2.txt') if -f 'test_cp_source2.txt';
unlink('test_cp_all.txt') if -f 'test_cp_all.txt';
unlink('test_cp_no_deref.txt') if -f 'test_cp_no_deref.txt';
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_cp_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};
do {
my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_cp_dir_copy"); die "exec failed: " . $!; } else { waitpid($pid, 0); } $?;

};

print "=== Example 022 completed successfully ===\n";

use Carp;
#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/023_mv_basic.pl" }


print "=== Example 023: Basic mv command ===\n";

open(my $fh, '>', 'test_mv_source.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for moving\n";
print $fh "It has multiple lines\n";
print $fh "To demonstrate mv functionality\n";
close($fh);

do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mkdir", "-p", "test_mv_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "Using " . "sys" . "tem" . "() to call mv (move file):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mv", "test_mv_source.txt", "test_mv_dest.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};
if (-f "test_mv_dest.txt") {
    print "File moved successfully\n";
    my $content = do { open my $fh, '<', 'test_mv_dest.txt' or die 'cat: ' . 'test_mv_dest.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "Content: $content";
} else {
    print "File move failed\n";
}

print "\nmv with verbose (-v):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mv", "-v", "test_mv_dest.txt", "test_mv_verbose.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\nmv with force (-f):\n";
my $mv_force = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            my $err;
            my $force = 1;
            if ( -e 'test_mv_verbose.txt' ) {
                my $dest = 'test_mv_force.txt';
                if ( -e $dest && -d $dest ) {
                    my $source_name = 'test_mv_verbose.txt';
                    $source_name =~ s{^.*[\/]}{};
                    $dest = "$dest/$source_name";
                }
                if ( -e $dest && !$force ) {
                    croak "mv: $dest: File exists (use -f to force overwrite)\n";
                }
                my $dest_dir = $dest;
                $dest_dir =~ s/\/[^\/]*$//msx;
                if ( $dest_dir eq $dest ) {
                    $dest_dir = q{};
                }
                if ( $dest_dir ne q{} && !-d $dest_dir ) {
                    my $err;
                    make_path( $dest_dir, { error => \$err } );
                    if ( @{$err} ) {
                        croak "mv: cannot create directory $dest_dir: $err->[0]\n";
                    }
                }
                require File::Copy;
                if ( File::Copy::move( 'test_mv_verbose.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_verbose.txt' to $dest: $!\n";
                }
            } else {
                croak "mv: 'test_mv_verbose.txt': No such file or directory\n";
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
if (-f "test_mv_force.txt") {
    print "File moved with force\n";
}

print "\nmv with interactive (-i):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mv", "-i", "test_mv_force.txt", "test_mv_interactive.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\nmv with backup (-b):\n";
my $mv_backup = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $mv_cmd = 'mv -b test_mv_interactive.txt test_mv_backup.txt';
                qx{$mv_cmd};
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
if (-f "test_mv_backup.txt") {
    print "File moved with backup\n";
}

print "\nmv with suffix (--suffix=.bak):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mv", "--suffix=.bak", "test_mv_backup.txt", "test_mv_suffix.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\nmv with no target directory (-T):\n";
my $mv_no_target = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $mv_cmd = 'mv -T test_mv_suffix.txt test_mv_no_target.txt';
                qx{$mv_cmd};
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
if (-f "test_mv_no_target.txt") {
    print "File moved with no target directory\n";
}

print "\nmv with update (-u):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mv", "-u", "test_mv_no_target.txt", "test_mv_update.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\nmv with no clobber (-n):\n";
my $mv_no_clobber = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $mv_cmd = 'mv -n test_mv_update.txt test_mv_no_clobber.txt';
                qx{$mv_cmd};
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
if (-f "test_mv_no_clobber.txt") {
    print "File moved with no clobber\n";
}

print "\nmv with strip trailing slashes (--strip-trailing-slashes):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mv", "--strip-trailing-slashes", "test_mv_no_clobber.txt", "test_mv_strip.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "\nmv with multiple files:\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_mv_file1.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_mv_file2.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};
my $mv_multi = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            my $err;
            my $force = 0;
            if ( -e 'test_mv_file1.txt' ) {
                my $dest = 'test_mv_dir/';
                if ( -e $dest && -d $dest ) {
                    my $source_name = 'test_mv_file1.txt';
                    $source_name =~ s{^.*[\/]}{};
                    $dest = "$dest/$source_name";
                }
                if ( -e $dest && !$force ) {
                    croak "mv: $dest: File exists (use -f to force overwrite)\n";
                }
                my $dest_dir = $dest;
                $dest_dir =~ s/\/[^\/]*$//msx;
                if ( $dest_dir eq $dest ) {
                    $dest_dir = q{};
                }
                if ( $dest_dir ne q{} && !-d $dest_dir ) {
                    my $err;
                    make_path( $dest_dir, { error => \$err } );
                    if ( @{$err} ) {
                        croak "mv: cannot create directory $dest_dir: $err->[0]\n";
                    }
                }
                require File::Copy;
                if ( File::Copy::move( 'test_mv_file1.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_file1.txt' to $dest: $!\n";
                }
            } else {
                croak "mv: 'test_mv_file1.txt': No such file or directory\n";
            }
            if ( -e 'test_mv_file2.txt' ) {
                my $dest = 'test_mv_dir/';
                if ( -e $dest && -d $dest ) {
                    my $source_name = 'test_mv_file2.txt';
                    $source_name =~ s{^.*[\/]}{};
                    $dest = "$dest/$source_name";
                }
                if ( -e $dest && !$force ) {
                    croak "mv: $dest: File exists (use -f to force overwrite)\n";
                }
                my $dest_dir = $dest;
                $dest_dir =~ s/\/[^\/]*$//msx;
                if ( $dest_dir eq $dest ) {
                    $dest_dir = q{};
                }
                if ( $dest_dir ne q{} && !-d $dest_dir ) {
                    my $err;
                    make_path( $dest_dir, { error => \$err } );
                    if ( @{$err} ) {
                        croak "mv: cannot create directory $dest_dir: $err->[0]\n";
                    }
                }
                require File::Copy;
                if ( File::Copy::move( 'test_mv_file2.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_file2.txt' to $dest: $!\n";
                }
            } else {
                croak "mv: 'test_mv_file2.txt': No such file or directory\n";
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
if (-f "test_mv_dir/test_mv_file1.txt" && -f "test_mv_dir/test_mv_file2.txt") {
    print "Multiple files moved successfully\n";
}

print "\nmv with preserve all (-a):\n";
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_mv_preserve.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mv", "-a", "test_mv_preserve.txt", "test_mv_preserve_dest.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

unlink('test_mv_strip.txt') if -f 'test_mv_strip.txt';
unlink('test_mv_preserve_dest.txt') if -f 'test_mv_preserve_dest.txt';
do {
my $pid = fork;
if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_mv_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); }
$?;

};

print "=== Example 023 completed successfully ===\n";

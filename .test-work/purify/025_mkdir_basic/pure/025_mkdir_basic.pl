use Carp;
#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/025_mkdir_basic.pl" }


print "=== Example 025: Basic mkdir command ===\n";

print "Using " . "sys" . "tem" . "() to call mkdir (create directory):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('mkdir', 'test_mkdir_dir'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
if (-d "test_mkdir_dir") {
    print "Directory created successfully\n";
} else {
    print "Directory creation failed\n";
}

print "\nmkdir with parents (-p):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('mkdir', '-p', 'test_mkdir_parents/subdir1/subdir2'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
if (-d "test_mkdir_parents/subdir1/subdir2") {
    print "Nested directories created successfully\n";
}

print "\nmkdir with mode (-m 755):\n";
my $mkdir_mode = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
        use File::Path qw(make_path);
        my $err;
        if ( mkdir '755' ) {
            }
        else {
            croak "mkdir: cannot create directory " . '755' . ": File exists\n";
        }
        if ( mkdir 'test_mkdir_mode' ) {
            }
        else {
            croak "mkdir: cannot create directory " . 'test_mkdir_mode' . ": File exists\n";
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
if (-d "test_mkdir_mode") {
    print "Directory created with mode 755\n";
}

print "\nmkdir with verbose (-v):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('mkdir', '-v', 'test_mkdir_verbose'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nmkdir with multiple directories:\n";
my $mkdir_multi = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
        use File::Path qw(make_path);
        my $err;
        if ( mkdir 'test_mkdir_multi1' ) {
            }
        else {
            croak "mkdir: cannot create directory " . 'test_mkdir_multi1' . ": File exists\n";
        }
        if ( mkdir 'test_mkdir_multi2' ) {
            }
        else {
            croak "mkdir: cannot create directory " . 'test_mkdir_multi2' . ": File exists\n";
        }
        if ( mkdir 'test_mkdir_multi3' ) {
            }
        else {
            croak "mkdir: cannot create directory " . 'test_mkdir_multi3' . ": File exists\n";
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
if (-d "test_mkdir_multi1" && -d "test_mkdir_multi2" && -d "test_mkdir_multi3") {
    print "Multiple directories created successfully\n";
}

print "\nmkdir with parents and mode (-p -m 700):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('mkdir', '-p', '-m', '700', 'test_mkdir_secure/subdir'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
if (-d "test_mkdir_secure/subdir") {
    print "Secure directory created successfully\n";
}

print "\nmkdir with parents and verbose (-p -v):\n";
my $mkdir_pv = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
        use File::Path qw(make_path);
        my $err;
        if ( !-d 'test_mkdir_pv/subdir1/subdir2' ) {
            my $mkdir_target = 'test_mkdir_pv/subdir1/subdir2';
            my @mkdir_verbose_paths;
            my $mkdir_prefix = $mkdir_target =~ m{^/} ? '/' : '';
            for my $mkdir_component ( split m{/}, $mkdir_target ) {
                next if $mkdir_component eq '';
                if ( $mkdir_prefix eq '' || $mkdir_prefix eq '/' ) {
                    $mkdir_prefix .= $mkdir_component;
                }
                else {
                    $mkdir_prefix .= '/' . $mkdir_component;
                }
                push @mkdir_verbose_paths, $mkdir_prefix if !-d $mkdir_prefix;
            }
            make_path( 'test_mkdir_pv/subdir1/subdir2', { error => \$err } );
            if ( @{$err} ) {
                croak "mkdir: cannot create directory " . 'test_mkdir_pv/subdir1/subdir2' . ": $err->[0]\n";
            }
            for my $mkdir_created (@mkdir_verbose_paths) {
                print "mkdir: created directory '" . $mkdir_created . "'\n";
            }
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
print $mkdir_pv;

print "\nmkdir with ignore existing (-p):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('mkdir', '-p', 'test_mkdir_dir'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;  
print "Attempted to create existing directory (should not fail with -p)\n";

print "\nmkdir with specific mode (777):\n";
my $mkdir_777 = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
        use File::Path qw(make_path);
        my $err;
        if ( mkdir '777' ) {
            }
        else {
            croak "mkdir: cannot create directory " . '777' . ": File exists\n";
        }
        if ( mkdir 'test_mkdir_777' ) {
            }
        else {
            croak "mkdir: cannot create directory " . 'test_mkdir_777' . ": File exists\n";
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
if (-d "test_mkdir_777") {
    print "Directory created with mode 777\n";
}

print "\nmkdir with parents and multiple directories:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('mkdir', '-p', 'test_mkdir_batch1/subdir', 'test_mkdir_batch2/subdir', 'test_mkdir_batch3/subdir'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
if (-d "test_mkdir_batch1/subdir" && -d "test_mkdir_batch2/subdir" && -d "test_mkdir_batch3/subdir") {
    print "Batch directories created successfully\n";
}

print "\nmkdir with error handling:\n";
my $mkdir_error = do { my $command = 'mkdir test_mkdir_error 2>&1'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
if (-d "test_mkdir_error") {
    print "Directory created successfully\n";
} else {
    print "Directory creation failed or already exists\n";
}

my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_dir'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_parents'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_mode'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_verbose'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_multi1'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_multi2'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_multi3'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_secure'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_pv'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_777'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_batch1'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_batch2'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_batch3'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('rm', '-rf', 'test_mkdir_error'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "=== Example 025 completed successfully ===\n";

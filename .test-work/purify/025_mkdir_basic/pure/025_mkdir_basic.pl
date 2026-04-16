#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/025_mkdir_basic.pl" }


print "=== Example 025: Basic mkdir command ===\n";

print "Using " . "sys" . "tem" . "() to call mkdir (create directory):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( mkdir 'test_mkdir_dir' ) {
    }
else {
    croak "mkdir: cannot create directory " . 'test_mkdir_dir' . ": File exists\n";
}

};
if (-d "test_mkdir_dir") {
    print "Directory created successfully\n";
} else {
    print "Directory creation failed\n";
}

print "\nmkdir with parents (-p):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_mkdir_parents/subdir1/subdir2' ) {
    make_path( 'test_mkdir_parents/subdir1/subdir2', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_mkdir_parents/subdir1/subdir2' . ": $err->[0]\n";
    }
}

};
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
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( mkdir 'test_mkdir_verbose' ) {
    print "mkdir: created directory '" . 'test_mkdir_verbose' . "'\n";
    }
else {
    croak "mkdir: cannot create directory " . 'test_mkdir_verbose' . ": File exists\n";
}

};

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
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
my $MAGIC_700 = 700;
use File::Path qw(make_path);
my $err;
if ( !-d '700' ) {
    make_path( '700', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . '700' . ": $err->[0]\n";
    }
}
if ( !-d 'test_mkdir_secure/subdir' ) {
    make_path( 'test_mkdir_secure/subdir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_mkdir_secure/subdir' . ": $err->[0]\n";
    }
}

};
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
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_mkdir_dir' ) {
    make_path( 'test_mkdir_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_mkdir_dir' . ": $err->[0]\n";
    }
}

};  
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
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_mkdir_batch1/subdir' ) {
    make_path( 'test_mkdir_batch1/subdir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_mkdir_batch1/subdir' . ": $err->[0]\n";
    }
}
if ( !-d 'test_mkdir_batch2/subdir' ) {
    make_path( 'test_mkdir_batch2/subdir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_mkdir_batch2/subdir' . ": $err->[0]\n";
    }
}
if ( !-d 'test_mkdir_batch3/subdir' ) {
    make_path( 'test_mkdir_batch3/subdir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_mkdir_batch3/subdir' . ": $err->[0]\n";
    }
}

};
if (-d "test_mkdir_batch1/subdir" && -d "test_mkdir_batch2/subdir" && -d "test_mkdir_batch3/subdir") {
    print "Batch directories created successfully\n";
}

print "\nmkdir with error handling:\n";
my $mkdir_error = do { my $command = 'mkdir test_mkdir_error 2> 1'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
if (-d "test_mkdir_error") {
    print "Directory created successfully\n";
} else {
    print "Directory creation failed or already exists\n";
}

do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_dir" ) {
    if ( -d "test_mkdir_dir" ) {
        my $err;
        remove_tree("test_mkdir_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_dir", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_dir" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_dir",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_parents" ) {
    if ( -d "test_mkdir_parents" ) {
        my $err;
        remove_tree("test_mkdir_parents", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_parents", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_parents" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_parents",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_mode" ) {
    if ( -d "test_mkdir_mode" ) {
        my $err;
        remove_tree("test_mkdir_mode", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_mode", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_mode" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_mode",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_verbose" ) {
    if ( -d "test_mkdir_verbose" ) {
        my $err;
        remove_tree("test_mkdir_verbose", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_verbose", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_verbose" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_verbose",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_multi1" ) {
    if ( -d "test_mkdir_multi1" ) {
        my $err;
        remove_tree("test_mkdir_multi1", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_multi1", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_multi1" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_multi1",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_multi2" ) {
    if ( -d "test_mkdir_multi2" ) {
        my $err;
        remove_tree("test_mkdir_multi2", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_multi2", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_multi2" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_multi2",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_multi3" ) {
    if ( -d "test_mkdir_multi3" ) {
        my $err;
        remove_tree("test_mkdir_multi3", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_multi3", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_multi3" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_multi3",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_secure" ) {
    if ( -d "test_mkdir_secure" ) {
        my $err;
        remove_tree("test_mkdir_secure", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_secure", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_secure" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_secure",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_pv" ) {
    if ( -d "test_mkdir_pv" ) {
        my $err;
        remove_tree("test_mkdir_pv", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_pv", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_pv" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_pv",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_777" ) {
    if ( -d "test_mkdir_777" ) {
        my $err;
        remove_tree("test_mkdir_777", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_777", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_777" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_777",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_batch1" ) {
    if ( -d "test_mkdir_batch1" ) {
        my $err;
        remove_tree("test_mkdir_batch1", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_batch1", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_batch1" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_batch1",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_batch2" ) {
    if ( -d "test_mkdir_batch2" ) {
        my $err;
        remove_tree("test_mkdir_batch2", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_batch2", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_batch2" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_batch2",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_batch3" ) {
    if ( -d "test_mkdir_batch3" ) {
        my $err;
        remove_tree("test_mkdir_batch3", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_batch3", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_batch3" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_batch3",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mkdir_error" ) {
    if ( -d "test_mkdir_error" ) {
        my $err;
        remove_tree("test_mkdir_error", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mkdir_error", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mkdir_error" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mkdir_error",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};

print "=== Example 025 completed successfully ===\n";

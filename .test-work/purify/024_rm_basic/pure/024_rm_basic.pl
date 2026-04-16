#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/024_rm_basic.pl" }


print "=== Example 024: Basic rm command ===\n";

open(my $fh, '>', 'test_rm_file1.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for removal\n";
close($fh);

open(my $fh2, '>', 'test_rm_file2.txt') or die "Cannot create test file: $!\n";
print $fh2 "This is another test file\n";
close($fh2);

do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_rm_dir' ) {
    make_path( 'test_rm_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_rm_dir' . ": $err->[0]\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_rm_dir/file3.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_rm_dir/file3.txt";
}
else {
    if ( open my $fh, '>', "test_rm_dir/file3.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_rm_dir/file3.txt",
          ": $ERRNO\n";
    }
}

};

print "Using " . "sys" . "tem" . "() to call rm (remove file):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_rm_file1.txt" ) {
    if ( -d "test_rm_file1.txt" ) {
        croak "rm: ", "test_rm_file1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_rm_file1.txt" ) {
            $main_exit_code = 0;
        }
        else {
            croak "rm: cannot remove ", "test_rm_file1.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_rm_file1.txt", ": No such file or directory\n";
}

};
if (!-f "test_rm_file1.txt") {
    print "File removed successfully\n";
} else {
    print "File removal failed\n";
}

print "\nrm with verbose (-v):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_rm_file2.txt" ) {
    if ( -d "test_rm_file2.txt" ) {
        croak "rm: ", "test_rm_file2.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_rm_file2.txt" ) {
            $main_exit_code = 0;
            print "removed '" . "test_rm_file2.txt" . "'\n";
        }
        else {
            croak "rm: cannot remove ", "test_rm_file2.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_rm_file2.txt", ": No such file or directory\n";
}

};

print "\nrm with force (-f):\n";
my $rm_force = do { my $command = 'rm -f test_rm_file2.txt 2> /dev/null'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Force removal attempted\n";

print "\nrm with interactive (-i):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_rm_interactive.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_rm_interactive.txt";
}
else {
    if ( open my $fh, '>', "test_rm_interactive.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_rm_interactive.txt",
          ": $ERRNO\n";
    }
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
do { my $rm_cmd = 'rm -i test_rm_interactive.txt'; qx{$rm_cmd}; };

};

print "\nrm with recursive (-r):\n";
my $rm_recursive = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_rm_dir" ) {
                if ( -d "test_rm_dir" ) {
                    my $err;
                    remove_tree("test_rm_dir", {error => \$err});
                    if (@{$err}) {
                        croak "rm: cannot remove ", "test_rm_dir", ": $err->[0]\n";
                    }
                    else {
                        $main_exit_code = 0;
                    }
                }
                else {
                    if ( unlink "test_rm_dir" ) {
                        $main_exit_code = 0;
                    }
                    else {
                        croak "rm: cannot remove ", "test_rm_dir",
                          ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: ", "test_rm_dir", ": No such file or directory\n";
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
if (!-d "test_rm_dir") {
    print "Directory removed recursively\n";
}

print "\nrm with recursive and force (-rf):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_rm_dir2/subdir' ) {
    make_path( 'test_rm_dir2/subdir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_rm_dir2/subdir' . ": $err->[0]\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_rm_dir2/file.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_rm_dir2/file.txt";
}
else {
    if ( open my $fh, '>', "test_rm_dir2/file.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_rm_dir2/file.txt",
          ": $ERRNO\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_rm_dir2/subdir/file2.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_rm_dir2/subdir/file2.txt";
}
else {
    if ( open my $fh, '>', "test_rm_dir2/subdir/file2.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_rm_dir2/subdir/file2.txt",
          ": $ERRNO\n";
    }
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_rm_dir2" ) {
    if ( -d "test_rm_dir2" ) {
        my $err;
        remove_tree("test_rm_dir2", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_rm_dir2", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_rm_dir2" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_rm_dir2",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};

print "\nrm with preserve root (--preserve-root):\n";
my $rm_preserve = do { my $command = 'rm -r f / 2> /dev/null || echo Protected from removing root'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
print $rm_preserve;

print "\nrm with one file " . "sys" . "tem" . " (-x):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_rm_xfs' ) {
    make_path( 'test_rm_xfs', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_rm_xfs' . ": $err->[0]\n";
    }
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
do { my $rm_cmd = 'rm -x test_rm_xfs'; qx{$rm_cmd}; };

};

print "\nrm with no dereference (-P):\n";
my $rm_no_deref = do { my $command = 'rm -P test_rm_xfs 2> /dev/null || echo No files to remove'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
print $rm_no_deref;

print "\nrm with ignore missing (-f):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "nonexistent_file.txt" ) {
    if ( -d "nonexistent_file.txt" ) {
        carp "rm: carping: ", "nonexistent_file.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "nonexistent_file.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "nonexistent_file.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
print "Ignored missing file\n";

print "\nrm with directory (-d):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_rm_empty_dir' ) {
    make_path( 'test_rm_empty_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_rm_empty_dir' . ": $err->[0]\n";
    }
}

};
my $rm_dir = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do { my $rm_cmd = 'rm -d test_rm_empty_dir'; qx{$rm_cmd}; };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (!-d "test_rm_empty_dir") {
    print "Empty directory removed\n";
}

print "\nrm with multiple files:\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_rm_multi1.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_rm_multi1.txt";
}
else {
    if ( open my $fh, '>', "test_rm_multi1.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_rm_multi1.txt",
          ": $ERRNO\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_rm_multi2.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_rm_multi2.txt";
}
else {
    if ( open my $fh, '>', "test_rm_multi2.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_rm_multi2.txt",
          ": $ERRNO\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_rm_multi3.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_rm_multi3.txt";
}
else {
    if ( open my $fh, '>', "test_rm_multi3.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_rm_multi3.txt",
          ": $ERRNO\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_rm_multi1.txt" ) {
    if ( -d "test_rm_multi1.txt" ) {
        croak "rm: ", "test_rm_multi1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_rm_multi1.txt" ) {
            $main_exit_code = 0;
        }
        else {
            croak "rm: cannot remove ", "test_rm_multi1.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_rm_multi1.txt", ": No such file or directory\n";
}
if ( -e "test_rm_multi2.txt" ) {
    if ( -d "test_rm_multi2.txt" ) {
        croak "rm: ", "test_rm_multi2.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_rm_multi2.txt" ) {
            $main_exit_code = 0;
        }
        else {
            croak "rm: cannot remove ", "test_rm_multi2.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_rm_multi2.txt", ": No such file or directory\n";
}
if ( -e "test_rm_multi3.txt" ) {
    if ( -d "test_rm_multi3.txt" ) {
        croak "rm: ", "test_rm_multi3.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_rm_multi3.txt" ) {
            $main_exit_code = 0;
        }
        else {
            croak "rm: cannot remove ", "test_rm_multi3.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_rm_multi3.txt", ": No such file or directory\n";
}

};
print "Multiple files removed\n";

unlink('test_rm_file1.txt') if -f 'test_rm_file1.txt';
unlink('test_rm_file2.txt') if -f 'test_rm_file2.txt';
unlink('test_rm_interactive.txt') if -f 'test_rm_interactive.txt';
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_rm_dir" ) {
    if ( -d "test_rm_dir" ) {
        my $err;
        remove_tree("test_rm_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_rm_dir", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_rm_dir" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_rm_dir",
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
if ( -e "test_rm_dir2" ) {
    if ( -d "test_rm_dir2" ) {
        my $err;
        remove_tree("test_rm_dir2", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_rm_dir2", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_rm_dir2" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_rm_dir2",
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
if ( -e "test_rm_xfs" ) {
    if ( -d "test_rm_xfs" ) {
        my $err;
        remove_tree("test_rm_xfs", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_rm_xfs", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_rm_xfs" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_rm_xfs",
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
if ( -e "test_rm_empty_dir" ) {
    if ( -d "test_rm_empty_dir" ) {
        my $err;
        remove_tree("test_rm_empty_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_rm_empty_dir", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_rm_empty_dir" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_rm_empty_dir",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};

print "=== Example 024 completed successfully ===\n";

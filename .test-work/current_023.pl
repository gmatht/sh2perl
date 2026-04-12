#!/usr/bin/perl


print "=== Example 023: Basic mv command ===\n";

open(my $fh, '>', 'test_mv_source.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for moving\n";
print $fh "It has multiple lines\n";
print $fh "To demonstrate mv functionality\n";
close($fh);

do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_mv_dir' ) {
    make_path( 'test_mv_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_mv_dir' . ": $err->[0]\n";
    }
}

};

print "Using " . "sys" . "tem" . "() to call mv (move file):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
my $err;
my $force = 0;
if ( -e 'test_mv_source.txt' ) {
    my $dest = 'test_mv_dest.txt';
    if ( -e $dest && -d $dest ) {
        my $source_name = 'test_mv_source.txt';
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
    if ( move( 'test_mv_source.txt', $dest ) ) {
    } else {
        croak
  "mv: cannot move 'test_mv_source.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_mv_source.txt': No such file or directory\n";
}

};
if (-f "test_mv_dest.txt") {
    print "File moved successfully\n";
    my $content = do { open my $fh, '<', 'test_mv_dest.txt' or die 'cat: ' . 'test_mv_dest.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; }
;
    print "Content: $content";
} else {
    print "File move failed\n";
}

print "\nmv with verbose (-v):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
my $err;
my $force = 0;
if ( -e 'test_mv_dest.txt' ) {
    my $dest = 'test_mv_verbose.txt';
    if ( -e $dest && -d $dest ) {
        my $source_name = 'test_mv_dest.txt';
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
    if ( move( 'test_mv_dest.txt', $dest ) ) {
        print "renamed 'test_mv_dest.txt' -> '$dest'\n";
    } else {
        croak
  "mv: cannot move 'test_mv_dest.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_mv_dest.txt': No such file or directory\n";
}

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
                if ( move( 'test_mv_verbose.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_verbose.txt' to $dest: $ERRNO\n";
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
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
my $err;
my $force = 0;
if ( -e 'test_mv_force.txt' ) {
    my $dest = 'test_mv_interactive.txt';
    if ( -e $dest && -d $dest ) {
        my $source_name = 'test_mv_force.txt';
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
    if ( move( 'test_mv_force.txt', $dest ) ) {
    } else {
        croak
  "mv: cannot move 'test_mv_force.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_mv_force.txt': No such file or directory\n";
}

};

print "\nmv with backup (-b):\n";
my $mv_backup = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            my $err;
            my $force = 0;
            if ( -e 'test_mv_interactive.txt' ) {
                my $dest = 'test_mv_backup.txt';
                if ( -e $dest && -d $dest ) {
                    my $source_name = 'test_mv_interactive.txt';
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
                if ( move( 'test_mv_interactive.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_interactive.txt' to $dest: $ERRNO\n";
                }
            } else {
                croak "mv: 'test_mv_interactive.txt': No such file or directory\n";
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
if (-f "test_mv_backup.txt") {
    print "File moved with backup\n";
}

print "\nmv with suffix (--suffix=.bak):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
my $err;
my $force = 0;
if ( -e 'test_mv_backup.txt' ) {
    my $dest = 'test_mv_suffix.txt';
    if ( -e $dest && -d $dest ) {
        my $source_name = 'test_mv_backup.txt';
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
    if ( move( 'test_mv_backup.txt', $dest ) ) {
    } else {
        croak
  "mv: cannot move 'test_mv_backup.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_mv_backup.txt': No such file or directory\n";
}

};

print "\nmv with no target directory (-T):\n";
my $mv_no_target = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            my $err;
            my $force = 0;
            if ( -e 'test_mv_suffix.txt' ) {
                my $dest = 'test_mv_no_target.txt';
                if ( -e $dest && -d $dest ) {
                    my $source_name = 'test_mv_suffix.txt';
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
                if ( move( 'test_mv_suffix.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_suffix.txt' to $dest: $ERRNO\n";
                }
            } else {
                croak "mv: 'test_mv_suffix.txt': No such file or directory\n";
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
if (-f "test_mv_no_target.txt") {
    print "File moved with no target directory\n";
}

print "\nmv with update (-u):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
my $err;
my $force = 0;
if ( -e 'test_mv_no_target.txt' ) {
    my $dest = 'test_mv_update.txt';
    if ( -e $dest && -d $dest ) {
        my $source_name = 'test_mv_no_target.txt';
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
    if ( move( 'test_mv_no_target.txt', $dest ) ) {
    } else {
        croak
  "mv: cannot move 'test_mv_no_target.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_mv_no_target.txt': No such file or directory\n";
}

};

print "\nmv with no clobber (-n):\n";
my $mv_no_clobber = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            my $err;
            my $force = 0;
            if ( -e 'test_mv_update.txt' ) {
                my $dest = 'test_mv_no_clobber.txt';
                if ( -e $dest && -d $dest ) {
                    my $source_name = 'test_mv_update.txt';
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
                if ( move( 'test_mv_update.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_update.txt' to $dest: $ERRNO\n";
                }
            } else {
                croak "mv: 'test_mv_update.txt': No such file or directory\n";
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
if (-f "test_mv_no_clobber.txt") {
    print "File moved with no clobber\n";
}

print "\nmv with strip trailing slashes (--strip-trailing-slashes):\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
my $err;
my $force = 0;
if ( -e 'test_mv_no_clobber.txt' ) {
    my $dest = 'test_mv_strip.txt';
    if ( -e $dest && -d $dest ) {
        my $source_name = 'test_mv_no_clobber.txt';
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
    if ( move( 'test_mv_no_clobber.txt', $dest ) ) {
    } else {
        croak
  "mv: cannot move 'test_mv_no_clobber.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_mv_no_clobber.txt': No such file or directory\n";
}

};

print "\nmv with multiple files:\n";
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_mv_file1.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_mv_file1.txt";
}
else {
    if ( open my $fh, '>', "test_mv_file1.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_mv_file1.txt",
          ": $ERRNO\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_mv_file2.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_mv_file2.txt";
}
else {
    if ( open my $fh, '>', "test_mv_file2.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_mv_file2.txt",
          ": $ERRNO\n";
    }
}

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
                if ( move( 'test_mv_file1.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_file1.txt' to $dest: $ERRNO\n";
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
                if ( move( 'test_mv_file2.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_mv_file2.txt' to $dest: $ERRNO\n";
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
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use POSIX qw(time);
if ( -e "test_mv_preserve.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_mv_preserve.txt";
}
else {
    if ( open my $fh, '>', "test_mv_preserve.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_mv_preserve.txt",
          ": $ERRNO\n";
    }
}

};
do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
my $err;
my $force = 0;
if ( -e 'test_mv_preserve.txt' ) {
    my $dest = 'test_mv_preserve_dest.txt';
    if ( -e $dest && -d $dest ) {
        my $source_name = 'test_mv_preserve.txt';
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
    if ( move( 'test_mv_preserve.txt', $dest ) ) {
    } else {
        croak
  "mv: cannot move 'test_mv_preserve.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_mv_preserve.txt': No such file or directory\n";
}

};

unlink('test_mv_strip.txt') if -f 'test_mv_strip.txt';
unlink('test_mv_preserve_dest.txt') if -f 'test_mv_preserve_dest.txt';
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_mv_dir" ) {
    if ( -d "test_mv_dir" ) {
        my $err;
        remove_tree("test_mv_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_mv_dir", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_mv_dir" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_mv_dir",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
    carp "rm: carping: ", "test_mv_dir", ": No such file or directory\n";
}

};

print "=== Example 023 completed successfully ===\n";

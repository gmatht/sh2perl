#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
use POSIX qw(time);

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

print "=== File Manipulation Commands ===\n";
print "=== cp command ===\n";
print "\n";
$CHILD_ERROR = 0;
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_file.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $cp_result = do {
    my $left_result_2 = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp test_file.txt test_file_copy.txt';
                my $cp_output = qx{$cp_cmd};
                # print $cp_output;
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_2 = do { ("Copy successful") };
        $left_result_2 . $right_result_2;
    } else {
        q{};
    }
};
do {
    my $output = "Copy result: $cp_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_3 = ();
my $ls_all_found_4 = 1;
my @ls_inputs_5 = ();
push @ls_inputs_5, 'test_file.txt';
push @ls_inputs_5, 'test_file_copy.txt';
push @ls_inputs_5, 'test_file_moved.txt';
my @ls_files_6 = ();
my @ls_dirs_7 = ();
my $ls_show_headers_8 = scalar(@ls_inputs_5) > 1;
for my $ls_item_9 (@ls_inputs_5) {
    if ( -f $ls_item_9 ) {
        push @ls_files_6, $ls_item_9;
    }
    elsif ( -d $ls_item_9 ) {
        push @ls_dirs_7, $ls_item_9;
    }
    else {
        $ls_all_found_4 = 0;
    }
}
@ls_files_6 = sort { $a cmp $b } @ls_files_6;
@ls_dirs_7 = sort { $a cmp $b } @ls_dirs_7;
if (@ls_files_6) {
    push @ls_files_3, join("\n", @ls_files_6);
}
for my $ls_dir_10 (@ls_dirs_7) {
    my @ls_dir_entries_11 = ();
    if ( opendir my $dh, $ls_dir_10 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_11, $file;
        }
        closedir $dh;
        @ls_dir_entries_11 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_11;
        if ( $ls_show_headers_8 ) {
            if ( @ls_dir_entries_11 ) {
                push @ls_files_3, $ls_dir_10 . ":\n" . join("\n", @ls_dir_entries_11);
            } else {
                push @ls_files_3, $ls_dir_10 . ':';
            }
        }
        elsif ( @ls_dir_entries_11 ) {
            push @ls_files_3, join("\n", @ls_dir_entries_11);
        }
    }
    else {
        $ls_all_found_4 = 0;
    }
}
if (@ls_files_3) {
    print join "\n\n", @ls_files_3;
    print "\n";
}
if ( $ls_all_found_4 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
print "\n";
$CHILD_ERROR = 0;
print "=== mv command ===\n";
my $mv_result = do {
    my $left_result_12 = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            my $err;
            my $force = 0;
            if ( -e 'test_file_copy.txt' ) {
                my $dest = 'test_file_moved.txt';
                if ( -e $dest && -d $dest ) {
                    my $source_name = 'test_file_copy.txt';
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
                if ( File::Copy::move( 'test_file_copy.txt', $dest ) ) {
                } else {
                    croak
              "mv: cannot move 'test_file_copy.txt' to $dest: $ERRNO\n";
                }
            } else {
                croak "mv: 'test_file_copy.txt': No such file or directory\n";
            }
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_12 = do { ("Move successful") };
        $left_result_12 . $right_result_12;
    } else {
        q{};
    }
};
do {
    my $output = "Move result: $mv_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_13 = ();
my $ls_all_found_14 = 1;
my @ls_inputs_15 = ();
push @ls_inputs_15, 'test_file.txt';
push @ls_inputs_15, 'test_file_copy.txt';
push @ls_inputs_15, 'test_file_moved.txt';
my @ls_files_16 = ();
my @ls_dirs_17 = ();
my $ls_show_headers_18 = scalar(@ls_inputs_15) > 1;
for my $ls_item_19 (@ls_inputs_15) {
    if ( -f $ls_item_19 ) {
        push @ls_files_16, $ls_item_19;
    }
    elsif ( -d $ls_item_19 ) {
        push @ls_dirs_17, $ls_item_19;
    }
    else {
        $ls_all_found_14 = 0;
    }
}
@ls_files_16 = sort { $a cmp $b } @ls_files_16;
@ls_dirs_17 = sort { $a cmp $b } @ls_dirs_17;
if (@ls_files_16) {
    push @ls_files_13, join("\n", @ls_files_16);
}
for my $ls_dir_20 (@ls_dirs_17) {
    my @ls_dir_entries_21 = ();
    if ( opendir my $dh, $ls_dir_20 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_21, $file;
        }
        closedir $dh;
        @ls_dir_entries_21 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_21;
        if ( $ls_show_headers_18 ) {
            if ( @ls_dir_entries_21 ) {
                push @ls_files_13, $ls_dir_20 . ":\n" . join("\n", @ls_dir_entries_21);
            } else {
                push @ls_files_13, $ls_dir_20 . ':';
            }
        }
        elsif ( @ls_dir_entries_21 ) {
            push @ls_files_13, join("\n", @ls_dir_entries_21);
        }
    }
    else {
        $ls_all_found_14 = 0;
    }
}
if (@ls_files_13) {
    print join "\n\n", @ls_files_13;
    print "\n";
}
if ( $ls_all_found_14 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
print "\n";
$CHILD_ERROR = 0;
print "=== rm command ===\n";
my $rm_result = do {
    my $left_result_22 = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_file.txt" ) {
                if ( -d "test_file.txt" ) {
                    croak "rm: ", "test_file.txt",
                      " is a directory (use -r to remove recursively)\n";
                }
                else {
                    if ( unlink "test_file.txt" ) {
                        $main_exit_code = 0;
                    }
                    else {
                        croak "rm: cannot remove ", "test_file.txt",
                          ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: ", "test_file.txt", ": No such file or directory\n";
            }
            if ( -e "test_file_moved.txt" ) {
                if ( -d "test_file_moved.txt" ) {
                    croak "rm: ", "test_file_moved.txt",
                      " is a directory (use -r to remove recursively)\n";
                }
                else {
                    if ( unlink "test_file_moved.txt" ) {
                        $main_exit_code = 0;
                    }
                    else {
                        croak "rm: cannot remove ", "test_file_moved.txt",
                          ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: ", "test_file_moved.txt", ": No such file or directory\n";
            }
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_22 = do { ("Remove successful") };
        $left_result_22 . $right_result_22;
    } else {
        q{};
    }
};
do {
    my $output = "Remove result: $rm_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_23 = ();
my $ls_all_found_24 = 1;
my @ls_inputs_25 = ();
push @ls_inputs_25, 'test_file.txt';
push @ls_inputs_25, 'test_file_copy.txt';
push @ls_inputs_25, 'test_file_moved.txt';
my @ls_files_26 = ();
my @ls_dirs_27 = ();
my $ls_show_headers_28 = scalar(@ls_inputs_25) > 1;
for my $ls_item_29 (@ls_inputs_25) {
    if ( -f $ls_item_29 ) {
        push @ls_files_26, $ls_item_29;
    }
    elsif ( -d $ls_item_29 ) {
        push @ls_dirs_27, $ls_item_29;
    }
    else {
        $ls_all_found_24 = 0;
    }
}
@ls_files_26 = sort { $a cmp $b } @ls_files_26;
@ls_dirs_27 = sort { $a cmp $b } @ls_dirs_27;
if (@ls_files_26) {
    push @ls_files_23, join("\n", @ls_files_26);
}
for my $ls_dir_30 (@ls_dirs_27) {
    my @ls_dir_entries_31 = ();
    if ( opendir my $dh, $ls_dir_30 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_31, $file;
        }
        closedir $dh;
        @ls_dir_entries_31 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_31;
        if ( $ls_show_headers_28 ) {
            if ( @ls_dir_entries_31 ) {
                push @ls_files_23, $ls_dir_30 . ":\n" . join("\n", @ls_dir_entries_31);
            } else {
                push @ls_files_23, $ls_dir_30 . ':';
            }
        }
        elsif ( @ls_dir_entries_31 ) {
            push @ls_files_23, join("\n", @ls_dir_entries_31);
        }
    }
    else {
        $ls_all_found_24 = 0;
    }
}
if (@ls_files_23) {
    print join "\n\n", @ls_files_23;
    print "\n";
}
if ( $ls_all_found_24 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
print "\n";
$CHILD_ERROR = 0;
print "=== mkdir command ===\n";
my $mkdir_result = do {
    my $left_result_32 = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
        use File::Path qw(make_path);
        if ( mkdir 'test_dir' ) {
            }
        else {
            croak "mkdir: cannot create directory " . 'test_dir' . ": File exists\n";
        }
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_32 = do { ("Directory created") };
        $left_result_32 . $right_result_32;
    } else {
        q{};
    }
};
do {
    my $output = "Mkdir result: $mkdir_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
if ( -e "test_dir/file" ) {
    my $current_time = time;
    utime $current_time, $current_time, "test_dir/file";
}
else {
    if ( open my $fh, '>', "test_dir/file" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "test_dir/file",
          ": $ERRNO\n";
    }
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_34 = ();
my $ls_all_found_35 = 1;
my @ls_inputs_36 = ();
push @ls_inputs_36, 'test_dir';
my @ls_files_37 = ();
my @ls_dirs_38 = ();
my $ls_show_headers_39 = scalar(@ls_inputs_36) > 1;
for my $ls_item_40 (@ls_inputs_36) {
    if ( -f $ls_item_40 ) {
        push @ls_files_37, $ls_item_40;
    }
    elsif ( -d $ls_item_40 ) {
        push @ls_dirs_38, $ls_item_40;
    }
    else {
        $ls_all_found_35 = 0;
    }
}
@ls_files_37 = sort { $a cmp $b } @ls_files_37;
@ls_dirs_38 = sort { $a cmp $b } @ls_dirs_38;
if (@ls_files_37) {
    push @ls_files_34, join("\n", @ls_files_37);
}
for my $ls_dir_41 (@ls_dirs_38) {
    my @ls_dir_entries_42 = ();
    if ( opendir my $dh, $ls_dir_41 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_42, $file;
        }
        closedir $dh;
        @ls_dir_entries_42 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_42;
        if ( $ls_show_headers_39 ) {
            if ( @ls_dir_entries_42 ) {
                push @ls_files_34, $ls_dir_41 . ":\n" . join("\n", @ls_dir_entries_42);
            } else {
                push @ls_files_34, $ls_dir_41 . ':';
            }
        }
        elsif ( @ls_dir_entries_42 ) {
            push @ls_files_34, join("\n", @ls_dir_entries_42);
        }
    }
    else {
        $ls_all_found_35 = 0;
    }
}
if (@ls_files_34) {
    print join "\n", @ls_files_34;
    print "\n";
}
if ( $ls_all_found_35 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "Directory not found\n";
}
if ( -e "test_dir/file" ) {
    if ( -d "test_dir/file" ) {
        croak "rm: ", "test_dir/file",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_dir/file" ) {
            $main_exit_code = 0;
        }
        else {
            croak "rm: cannot remove ", "test_dir/file",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_dir/file", ": No such file or directory\n";
}
if ( -d 'test_dir' ) {
    if ( rmdir 'test_dir' ) {
    }
    else {
        croak "rmdir: cannot remove directory 'test_dir': $ERRNO\n";
    }
}
else {
    croak "rmdir: 'test_dir': No such file or directory\n";
}
print "\n";
$CHILD_ERROR = 0;
print "=== touch command ===\n";
my $touch_result = do {
    my $left_result_43 = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_file.txt" ) {
                my $current_time = time;
                utime $current_time, $current_time, "test_file.txt";
            }
            else {
                if ( open my $fh, '>', "test_file.txt" ) {
                    close $fh or croak "Close failed: $ERRNO";
                }
                else {
                    croak "touch: cannot create ", "test_file.txt",
                      ": $ERRNO\n";
                }
            }
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_43 = do { ("File touched") };
        $left_result_43 . $right_result_43;
    } else {
        q{};
    }
};
do {
    my $output = "Touch result: $touch_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
print "\n";
$CHILD_ERROR = 0;
if ( -e "test_file.txt" ) {
    if ( -d "test_file.txt" ) {
        carp "rm: carping: ", "test_file.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_file.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_file.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "test_file_copy.txt" ) {
    if ( -d "test_file_copy.txt" ) {
        carp "rm: carping: ", "test_file_copy.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_file_copy.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_file_copy.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "test_file_moved.txt" ) {
    if ( -d "test_file_moved.txt" ) {
        carp "rm: carping: ", "test_file_moved.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_file_moved.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_file_moved.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
if ( -e "test_dir" ) {
    if ( -d "test_dir" ) {
        my $err;
        require File::Path;
        File::Path::remove_tree("test_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_dir", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_dir" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_dir",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ($CHILD_ERROR != 0) {
    1;
}

exit $main_exit_code;

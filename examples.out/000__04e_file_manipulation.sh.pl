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
our $CHILD_ERROR;

print "=== File Manipulation Commands ===\n";
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
    my $left_result_0 = do {
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
        my $right_result_0 = do { ("Copy successful") };
        $left_result_0 . $right_result_0;
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
my @ls_files_1 = ();
my $ls_all_found_2 = 1;
my @ls_inputs_3 = ();
push @ls_inputs_3, 'test_file.txt';
push @ls_inputs_3, 'test_file_copy.txt';
push @ls_inputs_3, 'test_file_moved.txt';
my @ls_files_4 = ();
my @ls_dirs_5 = ();
my $ls_show_headers_6 = scalar(@ls_inputs_3) > 1;
for my $ls_item_7 (@ls_inputs_3) {
    if ( -f $ls_item_7 ) {
        push @ls_files_4, $ls_item_7;
    }
    elsif ( -d $ls_item_7 ) {
        push @ls_dirs_5, $ls_item_7;
    }
    else {
        $ls_all_found_2 = 0;
    }
}
@ls_files_4 = sort { $a cmp $b } @ls_files_4;
@ls_dirs_5 = sort { $a cmp $b } @ls_dirs_5;
if (@ls_files_4) {
    push @ls_files_1, join("\n", @ls_files_4);
}
for my $ls_dir_8 (@ls_dirs_5) {
    my @ls_dir_entries_9 = ();
    if ( opendir my $dh, $ls_dir_8 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_9, $file;
        }
        closedir $dh;
        @ls_dir_entries_9 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_9;
        if ( $ls_show_headers_6 ) {
            if ( @ls_dir_entries_9 ) {
                push @ls_files_1, $ls_dir_8 . ":\n" . join("\n", @ls_dir_entries_9);
            } else {
                push @ls_files_1, $ls_dir_8 . ':';
            }
        }
        elsif ( @ls_dir_entries_9 ) {
            push @ls_files_1, join("\n", @ls_dir_entries_9);
        }
    }
    else {
        $ls_all_found_2 = 0;
    }
}
if (@ls_files_1) {
    print join "\n\n", @ls_files_1;
    print "\n";
}
if ( $ls_all_found_2 ) {
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
my $mv_result = do {
    my $left_result_10 = do {
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
        my $right_result_10 = do { ("Move successful") };
        $left_result_10 . $right_result_10;
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
my @ls_files_11 = ();
my $ls_all_found_12 = 1;
my @ls_inputs_13 = ();
push @ls_inputs_13, 'test_file.txt';
push @ls_inputs_13, 'test_file_copy.txt';
push @ls_inputs_13, 'test_file_moved.txt';
my @ls_files_14 = ();
my @ls_dirs_15 = ();
my $ls_show_headers_16 = scalar(@ls_inputs_13) > 1;
for my $ls_item_17 (@ls_inputs_13) {
    if ( -f $ls_item_17 ) {
        push @ls_files_14, $ls_item_17;
    }
    elsif ( -d $ls_item_17 ) {
        push @ls_dirs_15, $ls_item_17;
    }
    else {
        $ls_all_found_12 = 0;
    }
}
@ls_files_14 = sort { $a cmp $b } @ls_files_14;
@ls_dirs_15 = sort { $a cmp $b } @ls_dirs_15;
if (@ls_files_14) {
    push @ls_files_11, join("\n", @ls_files_14);
}
for my $ls_dir_18 (@ls_dirs_15) {
    my @ls_dir_entries_19 = ();
    if ( opendir my $dh, $ls_dir_18 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_19, $file;
        }
        closedir $dh;
        @ls_dir_entries_19 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_19;
        if ( $ls_show_headers_16 ) {
            if ( @ls_dir_entries_19 ) {
                push @ls_files_11, $ls_dir_18 . ":\n" . join("\n", @ls_dir_entries_19);
            } else {
                push @ls_files_11, $ls_dir_18 . ':';
            }
        }
        elsif ( @ls_dir_entries_19 ) {
            push @ls_files_11, join("\n", @ls_dir_entries_19);
        }
    }
    else {
        $ls_all_found_12 = 0;
    }
}
if (@ls_files_11) {
    print join "\n\n", @ls_files_11;
    print "\n";
}
if ( $ls_all_found_12 ) {
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
my $rm_result = do {
    my $left_result_20 = do {
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
        my $right_result_20 = do { ("Remove successful") };
        $left_result_20 . $right_result_20;
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
my @ls_files_21 = ();
my $ls_all_found_22 = 1;
my @ls_inputs_23 = ();
push @ls_inputs_23, 'test_file.txt';
push @ls_inputs_23, 'test_file_copy.txt';
push @ls_inputs_23, 'test_file_moved.txt';
my @ls_files_24 = ();
my @ls_dirs_25 = ();
my $ls_show_headers_26 = scalar(@ls_inputs_23) > 1;
for my $ls_item_27 (@ls_inputs_23) {
    if ( -f $ls_item_27 ) {
        push @ls_files_24, $ls_item_27;
    }
    elsif ( -d $ls_item_27 ) {
        push @ls_dirs_25, $ls_item_27;
    }
    else {
        $ls_all_found_22 = 0;
    }
}
@ls_files_24 = sort { $a cmp $b } @ls_files_24;
@ls_dirs_25 = sort { $a cmp $b } @ls_dirs_25;
if (@ls_files_24) {
    push @ls_files_21, join("\n", @ls_files_24);
}
for my $ls_dir_28 (@ls_dirs_25) {
    my @ls_dir_entries_29 = ();
    if ( opendir my $dh, $ls_dir_28 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_29, $file;
        }
        closedir $dh;
        @ls_dir_entries_29 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_29;
        if ( $ls_show_headers_26 ) {
            if ( @ls_dir_entries_29 ) {
                push @ls_files_21, $ls_dir_28 . ":\n" . join("\n", @ls_dir_entries_29);
            } else {
                push @ls_files_21, $ls_dir_28 . ':';
            }
        }
        elsif ( @ls_dir_entries_29 ) {
            push @ls_files_21, join("\n", @ls_dir_entries_29);
        }
    }
    else {
        $ls_all_found_22 = 0;
    }
}
if (@ls_files_21) {
    print join "\n\n", @ls_files_21;
    print "\n";
}
if ( $ls_all_found_22 ) {
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
my $mkdir_result = do {
    my $left_result_30 = do {
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
        my $right_result_30 = do { ("Directory created") };
        $left_result_30 . $right_result_30;
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
my @ls_files_32 = ();
my $ls_all_found_33 = 1;
my @ls_inputs_34 = ();
push @ls_inputs_34, 'test_dir';
my @ls_files_35 = ();
my @ls_dirs_36 = ();
my $ls_show_headers_37 = scalar(@ls_inputs_34) > 1;
for my $ls_item_38 (@ls_inputs_34) {
    if ( -f $ls_item_38 ) {
        push @ls_files_35, $ls_item_38;
    }
    elsif ( -d $ls_item_38 ) {
        push @ls_dirs_36, $ls_item_38;
    }
    else {
        $ls_all_found_33 = 0;
    }
}
@ls_files_35 = sort { $a cmp $b } @ls_files_35;
@ls_dirs_36 = sort { $a cmp $b } @ls_dirs_36;
if (@ls_files_35) {
    push @ls_files_32, join("\n", @ls_files_35);
}
for my $ls_dir_39 (@ls_dirs_36) {
    my @ls_dir_entries_40 = ();
    if ( opendir my $dh, $ls_dir_39 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_40, $file;
        }
        closedir $dh;
        @ls_dir_entries_40 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_40;
        if ( $ls_show_headers_37 ) {
            if ( @ls_dir_entries_40 ) {
                push @ls_files_32, $ls_dir_39 . ":\n" . join("\n", @ls_dir_entries_40);
            } else {
                push @ls_files_32, $ls_dir_39 . ':';
            }
        }
        elsif ( @ls_dir_entries_40 ) {
            push @ls_files_32, join("\n", @ls_dir_entries_40);
        }
    }
    else {
        $ls_all_found_33 = 0;
    }
}
if (@ls_files_32) {
    print join "\n", @ls_files_32;
    print "\n";
}
if ( $ls_all_found_33 ) {
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
my $touch_result = do {
    my $left_result_41 = do {
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
        my $right_result_41 = do { ("File touched") };
        $left_result_41 . $right_result_41;
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
print "=== File Manipulation Commands Complete ===\n";

exit $main_exit_code;

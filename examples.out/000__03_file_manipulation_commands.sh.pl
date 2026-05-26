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
    my $left_result_5 = do {
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
        my $right_result_5 = do { ("Copy successful") };
        $left_result_5 . $right_result_5;
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
my @ls_files_6 = ();
my $ls_all_found_7 = 1;
my @ls_inputs_8 = ();
push @ls_inputs_8, 'test_file.txt';
push @ls_inputs_8, 'test_file_copy.txt';
push @ls_inputs_8, 'test_file_moved.txt';
my @ls_files_9 = ();
my @ls_dirs_10 = ();
my $ls_show_headers_11 = scalar(@ls_inputs_8) > 1;
for my $ls_item_12 (@ls_inputs_8) {
    if ( -f $ls_item_12 ) {
        push @ls_files_9, $ls_item_12;
    }
    elsif ( -d $ls_item_12 ) {
        push @ls_dirs_10, $ls_item_12;
    }
    else {
        $ls_all_found_7 = 0;
    }
}
@ls_files_9 = sort { $a cmp $b } @ls_files_9;
@ls_dirs_10 = sort { $a cmp $b } @ls_dirs_10;
if (@ls_files_9) {
    push @ls_files_6, join("\n", @ls_files_9);
}
for my $ls_dir_13 (@ls_dirs_10) {
    my @ls_dir_entries_14 = ();
    if ( opendir my $dh, $ls_dir_13 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_14, $file;
        }
        closedir $dh;
        @ls_dir_entries_14 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_14;
        if ( $ls_show_headers_11 ) {
            if ( @ls_dir_entries_14 ) {
                push @ls_files_6, $ls_dir_13 . ":\n" . join("\n", @ls_dir_entries_14);
            } else {
                push @ls_files_6, $ls_dir_13 . ':';
            }
        }
        elsif ( @ls_dir_entries_14 ) {
            push @ls_files_6, join("\n", @ls_dir_entries_14);
        }
    }
    else {
        $ls_all_found_7 = 0;
    }
}
if (@ls_files_6) {
    print join "\n\n", @ls_files_6;
    print "\n";
}
if ( $ls_all_found_7 ) {
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
    my $left_result_15 = do {
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
        my $right_result_15 = do { ("Move successful") };
        $left_result_15 . $right_result_15;
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
my @ls_files_16 = ();
my $ls_all_found_17 = 1;
my @ls_inputs_18 = ();
push @ls_inputs_18, 'test_file.txt';
push @ls_inputs_18, 'test_file_copy.txt';
push @ls_inputs_18, 'test_file_moved.txt';
my @ls_files_19 = ();
my @ls_dirs_20 = ();
my $ls_show_headers_21 = scalar(@ls_inputs_18) > 1;
for my $ls_item_22 (@ls_inputs_18) {
    if ( -f $ls_item_22 ) {
        push @ls_files_19, $ls_item_22;
    }
    elsif ( -d $ls_item_22 ) {
        push @ls_dirs_20, $ls_item_22;
    }
    else {
        $ls_all_found_17 = 0;
    }
}
@ls_files_19 = sort { $a cmp $b } @ls_files_19;
@ls_dirs_20 = sort { $a cmp $b } @ls_dirs_20;
if (@ls_files_19) {
    push @ls_files_16, join("\n", @ls_files_19);
}
for my $ls_dir_23 (@ls_dirs_20) {
    my @ls_dir_entries_24 = ();
    if ( opendir my $dh, $ls_dir_23 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_24, $file;
        }
        closedir $dh;
        @ls_dir_entries_24 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_24;
        if ( $ls_show_headers_21 ) {
            if ( @ls_dir_entries_24 ) {
                push @ls_files_16, $ls_dir_23 . ":\n" . join("\n", @ls_dir_entries_24);
            } else {
                push @ls_files_16, $ls_dir_23 . ':';
            }
        }
        elsif ( @ls_dir_entries_24 ) {
            push @ls_files_16, join("\n", @ls_dir_entries_24);
        }
    }
    else {
        $ls_all_found_17 = 0;
    }
}
if (@ls_files_16) {
    print join "\n\n", @ls_files_16;
    print "\n";
}
if ( $ls_all_found_17 ) {
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
    my $left_result_25 = do {
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
        my $right_result_25 = do { ("Remove successful") };
        $left_result_25 . $right_result_25;
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
my @ls_files_26 = ();
my $ls_all_found_27 = 1;
my @ls_inputs_28 = ();
push @ls_inputs_28, 'test_file.txt';
push @ls_inputs_28, 'test_file_copy.txt';
push @ls_inputs_28, 'test_file_moved.txt';
my @ls_files_29 = ();
my @ls_dirs_30 = ();
my $ls_show_headers_31 = scalar(@ls_inputs_28) > 1;
for my $ls_item_32 (@ls_inputs_28) {
    if ( -f $ls_item_32 ) {
        push @ls_files_29, $ls_item_32;
    }
    elsif ( -d $ls_item_32 ) {
        push @ls_dirs_30, $ls_item_32;
    }
    else {
        $ls_all_found_27 = 0;
    }
}
@ls_files_29 = sort { $a cmp $b } @ls_files_29;
@ls_dirs_30 = sort { $a cmp $b } @ls_dirs_30;
if (@ls_files_29) {
    push @ls_files_26, join("\n", @ls_files_29);
}
for my $ls_dir_33 (@ls_dirs_30) {
    my @ls_dir_entries_34 = ();
    if ( opendir my $dh, $ls_dir_33 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_34, $file;
        }
        closedir $dh;
        @ls_dir_entries_34 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_34;
        if ( $ls_show_headers_31 ) {
            if ( @ls_dir_entries_34 ) {
                push @ls_files_26, $ls_dir_33 . ":\n" . join("\n", @ls_dir_entries_34);
            } else {
                push @ls_files_26, $ls_dir_33 . ':';
            }
        }
        elsif ( @ls_dir_entries_34 ) {
            push @ls_files_26, join("\n", @ls_dir_entries_34);
        }
    }
    else {
        $ls_all_found_27 = 0;
    }
}
if (@ls_files_26) {
    print join "\n\n", @ls_files_26;
    print "\n";
}
if ( $ls_all_found_27 ) {
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
    my $left_result_35 = do {
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
        my $right_result_35 = do { ("Directory created") };
        $left_result_35 . $right_result_35;
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
my @ls_files_37 = ();
my $ls_all_found_38 = 1;
my @ls_inputs_39 = ();
push @ls_inputs_39, 'test_dir';
my @ls_files_40 = ();
my @ls_dirs_41 = ();
my $ls_show_headers_42 = scalar(@ls_inputs_39) > 1;
for my $ls_item_43 (@ls_inputs_39) {
    if ( -f $ls_item_43 ) {
        push @ls_files_40, $ls_item_43;
    }
    elsif ( -d $ls_item_43 ) {
        push @ls_dirs_41, $ls_item_43;
    }
    else {
        $ls_all_found_38 = 0;
    }
}
@ls_files_40 = sort { $a cmp $b } @ls_files_40;
@ls_dirs_41 = sort { $a cmp $b } @ls_dirs_41;
if (@ls_files_40) {
    push @ls_files_37, join("\n", @ls_files_40);
}
for my $ls_dir_44 (@ls_dirs_41) {
    my @ls_dir_entries_45 = ();
    if ( opendir my $dh, $ls_dir_44 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_45, $file;
        }
        closedir $dh;
        @ls_dir_entries_45 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_45;
        if ( $ls_show_headers_42 ) {
            if ( @ls_dir_entries_45 ) {
                push @ls_files_37, $ls_dir_44 . ":\n" . join("\n", @ls_dir_entries_45);
            } else {
                push @ls_files_37, $ls_dir_44 . ':';
            }
        }
        elsif ( @ls_dir_entries_45 ) {
            push @ls_files_37, join("\n", @ls_dir_entries_45);
        }
    }
    else {
        $ls_all_found_38 = 0;
    }
}
if (@ls_files_37) {
    print join "\n", @ls_files_37;
    print "\n";
}
if ( $ls_all_found_38 ) {
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
    my $left_result_46 = do {
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
        my $right_result_46 = do { ("File touched") };
        $left_result_46 . $right_result_46;
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

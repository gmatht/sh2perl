#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
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
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'test_file.txt'
      or die "Cannot open file: $!\n";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
    0;
};
my $cp_result;
$cp_result = do {
    my $left_result_3 = do {
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
        my $right_result_3 = do { ("Copy successful") };
        $left_result_3 . $right_result_3;
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
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_4 = ();
my $ls_all_found_5 = 1;
my @ls_inputs_6 = ();
push @ls_inputs_6, 'test_file.txt';
push @ls_inputs_6, 'test_file_copy.txt';
push @ls_inputs_6, 'test_file_moved.txt';
my @ls_files_7 = ();
my @ls_dirs_8 = ();
my $ls_show_headers_9 = scalar(@ls_inputs_6) > 1;
for my $ls_item_10 (@ls_inputs_6) {
    if ( -f $ls_item_10 ) {
        push @ls_files_7, $ls_item_10;
    }
    elsif ( -d $ls_item_10 ) {
        push @ls_dirs_8, $ls_item_10;
    }
    else {
        $ls_all_found_5 = 0;
    }
}
@ls_files_7 = sort { $a cmp $b } @ls_files_7;
@ls_dirs_8 = sort { $a cmp $b } @ls_dirs_8;
if (@ls_files_7) {
    push @ls_files_4, join("\n", @ls_files_7);
}
for my $ls_dir_11 (@ls_dirs_8) {
    my @ls_dir_entries_12 = ();
    if ( opendir my $dh, $ls_dir_11 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_12, $file;
        }
        closedir $dh;
        @ls_dir_entries_12 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_12;
        if ( $ls_show_headers_9 ) {
            if ( @ls_dir_entries_12 ) {
                push @ls_files_4, $ls_dir_11 . ":\n" . join("\n", @ls_dir_entries_12);
            } else {
                push @ls_files_4, $ls_dir_11 . ':';
            }
        }
        elsif ( @ls_dir_entries_12 ) {
            push @ls_files_4, join("\n", @ls_dir_entries_12);
        }
    }
    else {
        $ls_all_found_5 = 0;
    }
}
if (@ls_files_4) {
    print join "\n\n", @ls_files_4;
    print "\n";
}
if ( $ls_all_found_5 ) {
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
my $mv_result;
$mv_result = do {
    my $left_result_13 = do {
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
                if ( rename( 'test_file_copy.txt', $dest ) ) {
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
        my $right_result_13 = do { ("Move successful") };
        $left_result_13 . $right_result_13;
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
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_14 = ();
my $ls_all_found_15 = 1;
my @ls_inputs_16 = ();
push @ls_inputs_16, 'test_file.txt';
push @ls_inputs_16, 'test_file_copy.txt';
push @ls_inputs_16, 'test_file_moved.txt';
my @ls_files_17 = ();
my @ls_dirs_18 = ();
my $ls_show_headers_19 = scalar(@ls_inputs_16) > 1;
for my $ls_item_20 (@ls_inputs_16) {
    if ( -f $ls_item_20 ) {
        push @ls_files_17, $ls_item_20;
    }
    elsif ( -d $ls_item_20 ) {
        push @ls_dirs_18, $ls_item_20;
    }
    else {
        $ls_all_found_15 = 0;
    }
}
@ls_files_17 = sort { $a cmp $b } @ls_files_17;
@ls_dirs_18 = sort { $a cmp $b } @ls_dirs_18;
if (@ls_files_17) {
    push @ls_files_14, join("\n", @ls_files_17);
}
for my $ls_dir_21 (@ls_dirs_18) {
    my @ls_dir_entries_22 = ();
    if ( opendir my $dh, $ls_dir_21 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_22, $file;
        }
        closedir $dh;
        @ls_dir_entries_22 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_22;
        if ( $ls_show_headers_19 ) {
            if ( @ls_dir_entries_22 ) {
                push @ls_files_14, $ls_dir_21 . ":\n" . join("\n", @ls_dir_entries_22);
            } else {
                push @ls_files_14, $ls_dir_21 . ':';
            }
        }
        elsif ( @ls_dir_entries_22 ) {
            push @ls_files_14, join("\n", @ls_dir_entries_22);
        }
    }
    else {
        $ls_all_found_15 = 0;
    }
}
if (@ls_files_14) {
    print join "\n\n", @ls_files_14;
    print "\n";
}
if ( $ls_all_found_15 ) {
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
my $rm_result;
$rm_result = do {
    my $left_result_23 = do {
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
                $CHILD_ERROR = 1;
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
                $CHILD_ERROR = 1;
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
        my $right_result_23 = do { ("Remove successful") };
        $left_result_23 . $right_result_23;
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
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_24 = ();
my $ls_all_found_25 = 1;
my @ls_inputs_26 = ();
push @ls_inputs_26, 'test_file.txt';
push @ls_inputs_26, 'test_file_copy.txt';
push @ls_inputs_26, 'test_file_moved.txt';
my @ls_files_27 = ();
my @ls_dirs_28 = ();
my $ls_show_headers_29 = scalar(@ls_inputs_26) > 1;
for my $ls_item_30 (@ls_inputs_26) {
    if ( -f $ls_item_30 ) {
        push @ls_files_27, $ls_item_30;
    }
    elsif ( -d $ls_item_30 ) {
        push @ls_dirs_28, $ls_item_30;
    }
    else {
        $ls_all_found_25 = 0;
    }
}
@ls_files_27 = sort { $a cmp $b } @ls_files_27;
@ls_dirs_28 = sort { $a cmp $b } @ls_dirs_28;
if (@ls_files_27) {
    push @ls_files_24, join("\n", @ls_files_27);
}
for my $ls_dir_31 (@ls_dirs_28) {
    my @ls_dir_entries_32 = ();
    if ( opendir my $dh, $ls_dir_31 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_32, $file;
        }
        closedir $dh;
        @ls_dir_entries_32 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_32;
        if ( $ls_show_headers_29 ) {
            if ( @ls_dir_entries_32 ) {
                push @ls_files_24, $ls_dir_31 . ":\n" . join("\n", @ls_dir_entries_32);
            } else {
                push @ls_files_24, $ls_dir_31 . ':';
            }
        }
        elsif ( @ls_dir_entries_32 ) {
            push @ls_files_24, join("\n", @ls_dir_entries_32);
        }
    }
    else {
        $ls_all_found_25 = 0;
    }
}
if (@ls_files_24) {
    print join "\n\n", @ls_files_24;
    print "\n";
}
if ( $ls_all_found_25 ) {
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
my $mkdir_result;
$mkdir_result = do {
    my $left_result_33 = do {
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
        my $right_result_33 = do { ("Directory created") };
        $left_result_33 . $right_result_33;
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
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_35 = ();
my $ls_all_found_36 = 1;
my @ls_inputs_37 = ();
push @ls_inputs_37, 'test_dir';
my @ls_files_38 = ();
my @ls_dirs_39 = ();
my $ls_show_headers_40 = scalar(@ls_inputs_37) > 1;
for my $ls_item_41 (@ls_inputs_37) {
    if ( -f $ls_item_41 ) {
        push @ls_files_38, $ls_item_41;
    }
    elsif ( -d $ls_item_41 ) {
        push @ls_dirs_39, $ls_item_41;
    }
    else {
        $ls_all_found_36 = 0;
    }
}
@ls_files_38 = sort { $a cmp $b } @ls_files_38;
@ls_dirs_39 = sort { $a cmp $b } @ls_dirs_39;
if (@ls_files_38) {
    push @ls_files_35, join("\n", @ls_files_38);
}
for my $ls_dir_42 (@ls_dirs_39) {
    my @ls_dir_entries_43 = ();
    if ( opendir my $dh, $ls_dir_42 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_43, $file;
        }
        closedir $dh;
        @ls_dir_entries_43 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_43;
        if ( $ls_show_headers_40 ) {
            if ( @ls_dir_entries_43 ) {
                push @ls_files_35, $ls_dir_42 . ":\n" . join("\n", @ls_dir_entries_43);
            } else {
                push @ls_files_35, $ls_dir_42 . ':';
            }
        }
        elsif ( @ls_dir_entries_43 ) {
            push @ls_files_35, join("\n", @ls_dir_entries_43);
        }
    }
    else {
        $ls_all_found_36 = 0;
    }
}
if (@ls_files_35) {
    print join "\n", @ls_files_35;
    print "\n";
}
if ( $ls_all_found_36 ) {
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
    $CHILD_ERROR = 1;
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
my $touch_result;
$touch_result = do {
    my $left_result_44 = do {
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
        my $right_result_44 = do { ("File touched") };
        $left_result_44 . $right_result_44;
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
    $CHILD_ERROR = 0;
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
    $CHILD_ERROR = 0;
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
    $CHILD_ERROR = 0;
}
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
if ( -e "test_dir" ) {
    if ( -d "test_dir" ) {
        my $err;
        require File::Path;
        File::Path::remove_tree("test_dir", {error => \$err});
        if (@{$err}) {
            $CHILD_ERROR = 1;
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
    $CHILD_ERROR = 0;
}
if ($CHILD_ERROR != 0) {
    1;
}

exit $main_exit_code;

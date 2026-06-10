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

$PROGRAM_NAME = '000__04e_file_manipulation.sh';
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
    my $left_result_64 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp test_file.txt test_file_copy.txt';
                my $cp_output = qx{$cp_cmd};
                # print $cp_output;
                $cp_output;
            };
            1;
            };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_64 = do { ("Copy successful") };
        $left_result_64 . $right_result_64;
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
my @ls_files_65 = ();
my $ls_all_found_66 = 1;
my @ls_inputs_67 = ();
push @ls_inputs_67, 'test_file.txt';
push @ls_inputs_67, 'test_file_copy.txt';
push @ls_inputs_67, 'test_file_moved.txt';
my @ls_files_68 = ();
my @ls_dirs_69 = ();
my $ls_show_headers_70 = scalar(@ls_inputs_67) > 1;
for my $ls_item_71 (@ls_inputs_67) {
    if ( -f $ls_item_71 ) {
        push @ls_files_68, $ls_item_71;
    }
    elsif ( -d $ls_item_71 ) {
        push @ls_dirs_69, $ls_item_71;
    }
    else {
        $ls_all_found_66 = 0;
    }
}
@ls_files_68 = sort { $a cmp $b } @ls_files_68;
@ls_dirs_69 = sort { $a cmp $b } @ls_dirs_69;
if (@ls_files_68) {
    push @ls_files_65, join("\n", @ls_files_68);
}
for my $ls_dir_72 (@ls_dirs_69) {
    my @ls_dir_entries_73 = ();
    if ( opendir my $dh, $ls_dir_72 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_73, $file;
        }
        closedir $dh;
        @ls_dir_entries_73 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_73;
        if ( $ls_show_headers_70 ) {
            if ( @ls_dir_entries_73 ) {
                push @ls_files_65, $ls_dir_72 . ":\n" . join("\n", @ls_dir_entries_73);
            } else {
                push @ls_files_65, $ls_dir_72 . ':';
            }
        }
        elsif ( @ls_dir_entries_73 ) {
            push @ls_files_65, join("\n", @ls_dir_entries_73);
        }
    }
    else {
        $ls_all_found_66 = 0;
    }
}
if (@ls_files_65) {
    print join "\n\n", @ls_files_65;
    print "\n";
}
if ( $ls_all_found_66 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
    $main_exit_code = $CHILD_ERROR;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
my $mv_result = do {
    my $left_result_74 = do {
        $CHILD_ERROR = 0;
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
            1;
            };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_74 = do { ("Move successful") };
        $left_result_74 . $right_result_74;
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
my @ls_files_75 = ();
my $ls_all_found_76 = 1;
my @ls_inputs_77 = ();
push @ls_inputs_77, 'test_file.txt';
push @ls_inputs_77, 'test_file_copy.txt';
push @ls_inputs_77, 'test_file_moved.txt';
my @ls_files_78 = ();
my @ls_dirs_79 = ();
my $ls_show_headers_80 = scalar(@ls_inputs_77) > 1;
for my $ls_item_81 (@ls_inputs_77) {
    if ( -f $ls_item_81 ) {
        push @ls_files_78, $ls_item_81;
    }
    elsif ( -d $ls_item_81 ) {
        push @ls_dirs_79, $ls_item_81;
    }
    else {
        $ls_all_found_76 = 0;
    }
}
@ls_files_78 = sort { $a cmp $b } @ls_files_78;
@ls_dirs_79 = sort { $a cmp $b } @ls_dirs_79;
if (@ls_files_78) {
    push @ls_files_75, join("\n", @ls_files_78);
}
for my $ls_dir_82 (@ls_dirs_79) {
    my @ls_dir_entries_83 = ();
    if ( opendir my $dh, $ls_dir_82 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_83, $file;
        }
        closedir $dh;
        @ls_dir_entries_83 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_83;
        if ( $ls_show_headers_80 ) {
            if ( @ls_dir_entries_83 ) {
                push @ls_files_75, $ls_dir_82 . ":\n" . join("\n", @ls_dir_entries_83);
            } else {
                push @ls_files_75, $ls_dir_82 . ':';
            }
        }
        elsif ( @ls_dir_entries_83 ) {
            push @ls_files_75, join("\n", @ls_dir_entries_83);
        }
    }
    else {
        $ls_all_found_76 = 0;
    }
}
if (@ls_files_75) {
    print join "\n\n", @ls_files_75;
    print "\n";
}
if ( $ls_all_found_76 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
    $main_exit_code = $CHILD_ERROR;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
my $rm_result = do {
    my $left_result_84 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_file.txt" ) {
                if ( -d "test_file.txt" ) {
                    croak "rm: ", "test_file.txt",
                      " is a directory (use -r to remove recursively)\n";
                }
                else {
                    if ( unlink "test_file.txt" ) {
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
            1;
            };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_84 = do { ("Remove successful") };
        $left_result_84 . $right_result_84;
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
my @ls_files_85 = ();
my $ls_all_found_86 = 1;
my @ls_inputs_87 = ();
push @ls_inputs_87, 'test_file.txt';
push @ls_inputs_87, 'test_file_copy.txt';
push @ls_inputs_87, 'test_file_moved.txt';
my @ls_files_88 = ();
my @ls_dirs_89 = ();
my $ls_show_headers_90 = scalar(@ls_inputs_87) > 1;
for my $ls_item_91 (@ls_inputs_87) {
    if ( -f $ls_item_91 ) {
        push @ls_files_88, $ls_item_91;
    }
    elsif ( -d $ls_item_91 ) {
        push @ls_dirs_89, $ls_item_91;
    }
    else {
        $ls_all_found_86 = 0;
    }
}
@ls_files_88 = sort { $a cmp $b } @ls_files_88;
@ls_dirs_89 = sort { $a cmp $b } @ls_dirs_89;
if (@ls_files_88) {
    push @ls_files_85, join("\n", @ls_files_88);
}
for my $ls_dir_92 (@ls_dirs_89) {
    my @ls_dir_entries_93 = ();
    if ( opendir my $dh, $ls_dir_92 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_93, $file;
        }
        closedir $dh;
        @ls_dir_entries_93 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_93;
        if ( $ls_show_headers_90 ) {
            if ( @ls_dir_entries_93 ) {
                push @ls_files_85, $ls_dir_92 . ":\n" . join("\n", @ls_dir_entries_93);
            } else {
                push @ls_files_85, $ls_dir_92 . ':';
            }
        }
        elsif ( @ls_dir_entries_93 ) {
            push @ls_files_85, join("\n", @ls_dir_entries_93);
        }
    }
    else {
        $ls_all_found_86 = 0;
    }
}
if (@ls_files_85) {
    print join "\n\n", @ls_files_85;
    print "\n";
}
if ( $ls_all_found_86 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
    $main_exit_code = $CHILD_ERROR;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
my $mkdir_result = do {
    my $left_result_94 = do { my $mkdir_cmd = 'mkdir test_dir'; my $mkdir_output = qx{$mkdir_cmd}; $CHILD_ERROR = $? >> 8; $mkdir_output; };
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_94 = do { ("Directory created") };
        $left_result_94 . $right_result_94;
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
my @ls_files_96 = ();
my $ls_all_found_97 = 1;
my @ls_inputs_98 = ();
push @ls_inputs_98, 'test_dir';
my @ls_files_99 = ();
my @ls_dirs_100 = ();
my $ls_show_headers_101 = scalar(@ls_inputs_98) > 1;
for my $ls_item_102 (@ls_inputs_98) {
    if ( -f $ls_item_102 ) {
        push @ls_files_99, $ls_item_102;
    }
    elsif ( -d $ls_item_102 ) {
        push @ls_dirs_100, $ls_item_102;
    }
    else {
        $ls_all_found_97 = 0;
    }
}
@ls_files_99 = sort { $a cmp $b } @ls_files_99;
@ls_dirs_100 = sort { $a cmp $b } @ls_dirs_100;
if (@ls_files_99) {
    push @ls_files_96, join("\n", @ls_files_99);
}
for my $ls_dir_103 (@ls_dirs_100) {
    my @ls_dir_entries_104 = ();
    if ( opendir my $dh, $ls_dir_103 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_104, $file;
        }
        closedir $dh;
        @ls_dir_entries_104 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_104;
        if ( $ls_show_headers_101 ) {
            if ( @ls_dir_entries_104 ) {
                push @ls_files_96, $ls_dir_103 . ":\n" . join("\n", @ls_dir_entries_104);
            } else {
                push @ls_files_96, $ls_dir_103 . ':';
            }
        }
        elsif ( @ls_dir_entries_104 ) {
            push @ls_files_96, join("\n", @ls_dir_entries_104);
        }
    }
    else {
        $ls_all_found_97 = 0;
    }
}
if (@ls_files_96) {
    print join "\n", @ls_files_96;
    print "\n";
}
if ( $ls_all_found_97 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
    $main_exit_code = $CHILD_ERROR;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "Directory not found\n";
}
my $touch_result = do {
    my $left_result_105 = do {
        $CHILD_ERROR = 0;
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
            $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_105 = do { ("File touched") };
        $left_result_105 . $right_result_105;
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
local *STDERR;
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
                    }
    }
    else {
        if ( unlink "test_dir" ) {
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

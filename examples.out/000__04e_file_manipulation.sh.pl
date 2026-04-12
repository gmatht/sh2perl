#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
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
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'test_file.txt'
      or die "Cannot open file: $!\n";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
my $cp_result = do {
    my $left_result_70 = do {
            local $CHILD_ERROR = 0;
            my $eval_result = eval {
                my $err;
                if ( -e 'test_file.txt' ) {
                    my $dest = 'test_file_copy.txt';
                    if ( -d $dest ) {
                        $dest = "$dest/'test_file.txt'";
                    }
                    if ( copy( 'test_file.txt', $dest ) ) {
                    }
                    else {
                        croak "cp: cannot copy 'test_file.txt' to $dest: $ERRNO\n";
                    }
                }
                else {
                    croak "cp: 'test_file.txt': No such file or directory\n";
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
        my $right_result_70 = ("Copy successful");
        $left_result_70 . $right_result_70;
    }
    else {
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
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_71 = ();
my $ls_all_found_72 = 1;
my @ls_inputs_73 = ();
push @ls_inputs_73, 'test_file.txt';
push @ls_inputs_73, 'test_file_copy.txt';
push @ls_inputs_73, 'test_file_moved.txt';
my @ls_files_74 = ();
my @ls_dirs_75 = ();
my $ls_show_headers_76 = scalar(@ls_inputs_73) > 1;
for my $ls_item_77 (@ls_inputs_73) {
    if ( -f $ls_item_77 ) {
        push @ls_files_74, $ls_item_77;
    }
    elsif ( -d $ls_item_77 ) {
        push @ls_dirs_75, $ls_item_77;
    }
    else {
        $ls_all_found_72 = 0;
    }
}
@ls_files_74 = sort { $a cmp $b } @ls_files_74;
@ls_dirs_75 = sort { $a cmp $b } @ls_dirs_75;
if (@ls_files_74) {
    push @ls_files_71, join("\n", @ls_files_74);
}
for my $ls_dir_78 (@ls_dirs_75) {
    my @ls_dir_entries_79 = ();
    if ( opendir my $dh, $ls_dir_78 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_79, $file;
        }
        closedir $dh;
        @ls_dir_entries_79 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_79;
        if ( $ls_show_headers_76 ) {
            if ( @ls_dir_entries_79 ) {
                push @ls_files_71, $ls_dir_78 . ":\n" . join("\n", @ls_dir_entries_79);
            } else {
                push @ls_files_71, $ls_dir_78 . ':';
            }
        }
        elsif ( @ls_dir_entries_79 ) {
            push @ls_files_71, join("\n", @ls_dir_entries_79);
        }
    }
    else {
        $ls_all_found_72 = 0;
    }
}
if (@ls_files_71) {
    print join "\n\n", @ls_files_71;
    print "\n";
}
if ( $ls_all_found_72 ) {
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
    my $left_result_80 = do {
            local $CHILD_ERROR = 0;
            my $eval_result = eval {
                my $force = 0;
                if ( -e 'test_file_copy.txt' ) {
                    my $dest = 'test_file_moved.txt';
                    if ( -e $dest && -d $dest ) {
                        $dest = "$dest/'test_file_copy.txt'";
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
                    if ( move( 'test_file_copy.txt', $dest ) ) {
                        # # print "mv: moved 'test_file_copy.txt' to $dest\n";
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
        my $right_result_80 = ("Move successful");
        $left_result_80 . $right_result_80;
    }
    else {
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
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_81 = ();
my $ls_all_found_82 = 1;
my @ls_inputs_83 = ();
push @ls_inputs_83, 'test_file.txt';
push @ls_inputs_83, 'test_file_copy.txt';
push @ls_inputs_83, 'test_file_moved.txt';
my @ls_files_84 = ();
my @ls_dirs_85 = ();
my $ls_show_headers_86 = scalar(@ls_inputs_83) > 1;
for my $ls_item_87 (@ls_inputs_83) {
    if ( -f $ls_item_87 ) {
        push @ls_files_84, $ls_item_87;
    }
    elsif ( -d $ls_item_87 ) {
        push @ls_dirs_85, $ls_item_87;
    }
    else {
        $ls_all_found_82 = 0;
    }
}
@ls_files_84 = sort { $a cmp $b } @ls_files_84;
@ls_dirs_85 = sort { $a cmp $b } @ls_dirs_85;
if (@ls_files_84) {
    push @ls_files_81, join("\n", @ls_files_84);
}
for my $ls_dir_88 (@ls_dirs_85) {
    my @ls_dir_entries_89 = ();
    if ( opendir my $dh, $ls_dir_88 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_89, $file;
        }
        closedir $dh;
        @ls_dir_entries_89 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_89;
        if ( $ls_show_headers_86 ) {
            if ( @ls_dir_entries_89 ) {
                push @ls_files_81, $ls_dir_88 . ":\n" . join("\n", @ls_dir_entries_89);
            } else {
                push @ls_files_81, $ls_dir_88 . ':';
            }
        }
        elsif ( @ls_dir_entries_89 ) {
            push @ls_files_81, join("\n", @ls_dir_entries_89);
        }
    }
    else {
        $ls_all_found_82 = 0;
    }
}
if (@ls_files_81) {
    print join "\n\n", @ls_files_81;
    print "\n";
}
if ( $ls_all_found_82 ) {
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
    my $left_result_90 = do {
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
        my $right_result_90 = ("Remove successful");
        $left_result_90 . $right_result_90;
    }
    else {
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
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_91 = ();
my $ls_all_found_92 = 1;
my @ls_inputs_93 = ();
push @ls_inputs_93, 'test_file.txt';
push @ls_inputs_93, 'test_file_copy.txt';
push @ls_inputs_93, 'test_file_moved.txt';
my @ls_files_94 = ();
my @ls_dirs_95 = ();
my $ls_show_headers_96 = scalar(@ls_inputs_93) > 1;
for my $ls_item_97 (@ls_inputs_93) {
    if ( -f $ls_item_97 ) {
        push @ls_files_94, $ls_item_97;
    }
    elsif ( -d $ls_item_97 ) {
        push @ls_dirs_95, $ls_item_97;
    }
    else {
        $ls_all_found_92 = 0;
    }
}
@ls_files_94 = sort { $a cmp $b } @ls_files_94;
@ls_dirs_95 = sort { $a cmp $b } @ls_dirs_95;
if (@ls_files_94) {
    push @ls_files_91, join("\n", @ls_files_94);
}
for my $ls_dir_98 (@ls_dirs_95) {
    my @ls_dir_entries_99 = ();
    if ( opendir my $dh, $ls_dir_98 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_99, $file;
        }
        closedir $dh;
        @ls_dir_entries_99 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_99;
        if ( $ls_show_headers_96 ) {
            if ( @ls_dir_entries_99 ) {
                push @ls_files_91, $ls_dir_98 . ":\n" . join("\n", @ls_dir_entries_99);
            } else {
                push @ls_files_91, $ls_dir_98 . ':';
            }
        }
        elsif ( @ls_dir_entries_99 ) {
            push @ls_files_91, join("\n", @ls_dir_entries_99);
        }
    }
    else {
        $ls_all_found_92 = 0;
    }
}
if (@ls_files_91) {
    print join "\n\n", @ls_files_91;
    print "\n";
}
if ( $ls_all_found_92 ) {
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
    my $left_result_100 = do {
            local $CHILD_ERROR = 0;
            my $eval_result = eval {
            use File::Path qw(make_path);
            if ( mkdir 'test_dir' ) {
                }
            else {
                croak "mkdir: cannot create directory 'test_dir': File exists\n";
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
        my $right_result_100 = ("Directory created");
        $left_result_100 . $right_result_100;
    }
    else {
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
my @ls_files_102 = ();
my $ls_all_found_103 = 1;
my @ls_inputs_104 = ();
push @ls_inputs_104, 'test_dir';
my @ls_files_105 = ();
my @ls_dirs_106 = ();
my $ls_show_headers_107 = scalar(@ls_inputs_104) > 1;
for my $ls_item_108 (@ls_inputs_104) {
    if ( -f $ls_item_108 ) {
        push @ls_files_105, $ls_item_108;
    }
    elsif ( -d $ls_item_108 ) {
        push @ls_dirs_106, $ls_item_108;
    }
    else {
        $ls_all_found_103 = 0;
    }
}
@ls_files_105 = sort { $a cmp $b } @ls_files_105;
@ls_dirs_106 = sort { $a cmp $b } @ls_dirs_106;
if (@ls_files_105) {
    push @ls_files_102, join("\n", @ls_files_105);
}
for my $ls_dir_109 (@ls_dirs_106) {
    my @ls_dir_entries_110 = ();
    if ( opendir my $dh, $ls_dir_109 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_110, $file;
        }
        closedir $dh;
        @ls_dir_entries_110 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_110;
        if ( $ls_show_headers_107 ) {
            if ( @ls_dir_entries_110 ) {
                push @ls_files_102, $ls_dir_109 . ":\n" . join("\n", @ls_dir_entries_110);
            } else {
                push @ls_files_102, $ls_dir_109 . ':';
            }
        }
        elsif ( @ls_dir_entries_110 ) {
            push @ls_files_102, join("\n", @ls_dir_entries_110);
        }
    }
    else {
        $ls_all_found_103 = 0;
    }
}
if (@ls_files_102) {
    print join "\n", @ls_files_102;
    print "\n";
}
if ( $ls_all_found_103 ) {
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
    my $left_result_111 = do {
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
        my $right_result_111 = ("File touched");
        $left_result_111 . $right_result_111;
    }
    else {
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
    carp "rm: carping: ", "test_file.txt", ": No such file or directory\n";
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
    carp "rm: carping: ", "test_file_copy.txt", ": No such file or directory\n";
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
    carp "rm: carping: ", "test_file_moved.txt", ": No such file or directory\n";
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
if ( -e "test_dir" ) {
    if ( -d "test_dir" ) {
        my $err;
        remove_tree("test_dir", {error => \$err});
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
    carp "rm: carping: ", "test_dir", ": No such file or directory\n";
}
if ($CHILD_ERROR != 0) {
    1;
}
print "=== File Manipulation Commands Complete ===\n";

exit $main_exit_code;

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
    my $left_result_67 = do {
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
        my $right_result_67 = do { ("Copy successful") };
        $left_result_67 . $right_result_67;
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
my @ls_files_68 = ();
my $ls_all_found_69 = 1;
my @ls_inputs_70 = ();
push @ls_inputs_70, 'test_file.txt';
push @ls_inputs_70, 'test_file_copy.txt';
push @ls_inputs_70, 'test_file_moved.txt';
my @ls_files_71 = ();
my @ls_dirs_72 = ();
my $ls_show_headers_73 = scalar(@ls_inputs_70) > 1;
for my $ls_item_74 (@ls_inputs_70) {
    if ( -f $ls_item_74 ) {
        push @ls_files_71, $ls_item_74;
    }
    elsif ( -d $ls_item_74 ) {
        push @ls_dirs_72, $ls_item_74;
    }
    else {
        $ls_all_found_69 = 0;
    }
}
@ls_files_71 = sort { $a cmp $b } @ls_files_71;
@ls_dirs_72 = sort { $a cmp $b } @ls_dirs_72;
if (@ls_files_71) {
    push @ls_files_68, join("\n", @ls_files_71);
}
for my $ls_dir_75 (@ls_dirs_72) {
    my @ls_dir_entries_76 = ();
    if ( opendir my $dh, $ls_dir_75 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_76, $file;
        }
        closedir $dh;
        @ls_dir_entries_76 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_76;
        if ( $ls_show_headers_73 ) {
            if ( @ls_dir_entries_76 ) {
                push @ls_files_68, $ls_dir_75 . ":\n" . join("\n", @ls_dir_entries_76);
            } else {
                push @ls_files_68, $ls_dir_75 . ':';
            }
        }
        elsif ( @ls_dir_entries_76 ) {
            push @ls_files_68, join("\n", @ls_dir_entries_76);
        }
    }
    else {
        $ls_all_found_69 = 0;
    }
}
if (@ls_files_68) {
    print join "\n\n", @ls_files_68;
    print "\n";
}
if ( $ls_all_found_69 ) {
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
    my $left_result_77 = do {
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
        my $right_result_77 = do { ("Move successful") };
        $left_result_77 . $right_result_77;
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
my @ls_files_78 = ();
my $ls_all_found_79 = 1;
my @ls_inputs_80 = ();
push @ls_inputs_80, 'test_file.txt';
push @ls_inputs_80, 'test_file_copy.txt';
push @ls_inputs_80, 'test_file_moved.txt';
my @ls_files_81 = ();
my @ls_dirs_82 = ();
my $ls_show_headers_83 = scalar(@ls_inputs_80) > 1;
for my $ls_item_84 (@ls_inputs_80) {
    if ( -f $ls_item_84 ) {
        push @ls_files_81, $ls_item_84;
    }
    elsif ( -d $ls_item_84 ) {
        push @ls_dirs_82, $ls_item_84;
    }
    else {
        $ls_all_found_79 = 0;
    }
}
@ls_files_81 = sort { $a cmp $b } @ls_files_81;
@ls_dirs_82 = sort { $a cmp $b } @ls_dirs_82;
if (@ls_files_81) {
    push @ls_files_78, join("\n", @ls_files_81);
}
for my $ls_dir_85 (@ls_dirs_82) {
    my @ls_dir_entries_86 = ();
    if ( opendir my $dh, $ls_dir_85 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_86, $file;
        }
        closedir $dh;
        @ls_dir_entries_86 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_86;
        if ( $ls_show_headers_83 ) {
            if ( @ls_dir_entries_86 ) {
                push @ls_files_78, $ls_dir_85 . ":\n" . join("\n", @ls_dir_entries_86);
            } else {
                push @ls_files_78, $ls_dir_85 . ':';
            }
        }
        elsif ( @ls_dir_entries_86 ) {
            push @ls_files_78, join("\n", @ls_dir_entries_86);
        }
    }
    else {
        $ls_all_found_79 = 0;
    }
}
if (@ls_files_78) {
    print join "\n\n", @ls_files_78;
    print "\n";
}
if ( $ls_all_found_79 ) {
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
    my $left_result_87 = do {
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
        my $right_result_87 = do { ("Remove successful") };
        $left_result_87 . $right_result_87;
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
my @ls_files_88 = ();
my $ls_all_found_89 = 1;
my @ls_inputs_90 = ();
push @ls_inputs_90, 'test_file.txt';
push @ls_inputs_90, 'test_file_copy.txt';
push @ls_inputs_90, 'test_file_moved.txt';
my @ls_files_91 = ();
my @ls_dirs_92 = ();
my $ls_show_headers_93 = scalar(@ls_inputs_90) > 1;
for my $ls_item_94 (@ls_inputs_90) {
    if ( -f $ls_item_94 ) {
        push @ls_files_91, $ls_item_94;
    }
    elsif ( -d $ls_item_94 ) {
        push @ls_dirs_92, $ls_item_94;
    }
    else {
        $ls_all_found_89 = 0;
    }
}
@ls_files_91 = sort { $a cmp $b } @ls_files_91;
@ls_dirs_92 = sort { $a cmp $b } @ls_dirs_92;
if (@ls_files_91) {
    push @ls_files_88, join("\n", @ls_files_91);
}
for my $ls_dir_95 (@ls_dirs_92) {
    my @ls_dir_entries_96 = ();
    if ( opendir my $dh, $ls_dir_95 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_96, $file;
        }
        closedir $dh;
        @ls_dir_entries_96 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_96;
        if ( $ls_show_headers_93 ) {
            if ( @ls_dir_entries_96 ) {
                push @ls_files_88, $ls_dir_95 . ":\n" . join("\n", @ls_dir_entries_96);
            } else {
                push @ls_files_88, $ls_dir_95 . ':';
            }
        }
        elsif ( @ls_dir_entries_96 ) {
            push @ls_files_88, join("\n", @ls_dir_entries_96);
        }
    }
    else {
        $ls_all_found_89 = 0;
    }
}
if (@ls_files_88) {
    print join "\n\n", @ls_files_88;
    print "\n";
}
if ( $ls_all_found_89 ) {
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
    my $left_result_97 = do {
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
        my $right_result_97 = do { ("Directory created") };
        $left_result_97 . $right_result_97;
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
my @ls_files_99 = ();
my $ls_all_found_100 = 1;
my @ls_inputs_101 = ();
push @ls_inputs_101, 'test_dir';
my @ls_files_102 = ();
my @ls_dirs_103 = ();
my $ls_show_headers_104 = scalar(@ls_inputs_101) > 1;
for my $ls_item_105 (@ls_inputs_101) {
    if ( -f $ls_item_105 ) {
        push @ls_files_102, $ls_item_105;
    }
    elsif ( -d $ls_item_105 ) {
        push @ls_dirs_103, $ls_item_105;
    }
    else {
        $ls_all_found_100 = 0;
    }
}
@ls_files_102 = sort { $a cmp $b } @ls_files_102;
@ls_dirs_103 = sort { $a cmp $b } @ls_dirs_103;
if (@ls_files_102) {
    push @ls_files_99, join("\n", @ls_files_102);
}
for my $ls_dir_106 (@ls_dirs_103) {
    my @ls_dir_entries_107 = ();
    if ( opendir my $dh, $ls_dir_106 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_107, $file;
        }
        closedir $dh;
        @ls_dir_entries_107 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_107;
        if ( $ls_show_headers_104 ) {
            if ( @ls_dir_entries_107 ) {
                push @ls_files_99, $ls_dir_106 . ":\n" . join("\n", @ls_dir_entries_107);
            } else {
                push @ls_files_99, $ls_dir_106 . ':';
            }
        }
        elsif ( @ls_dir_entries_107 ) {
            push @ls_files_99, join("\n", @ls_dir_entries_107);
        }
    }
    else {
        $ls_all_found_100 = 0;
    }
}
if (@ls_files_99) {
    print join "\n", @ls_files_99;
    print "\n";
}
if ( $ls_all_found_100 ) {
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
    my $left_result_108 = do {
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
        my $right_result_108 = do { ("File touched") };
        $left_result_108 . $right_result_108;
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

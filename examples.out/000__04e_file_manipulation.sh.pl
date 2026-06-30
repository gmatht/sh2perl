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
    my $left_result_68 = do {
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
        my $right_result_68 = do { ("Copy successful") };
        $left_result_68 . $right_result_68;
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
my @ls_files_69 = ();
my $ls_all_found_70 = 1;
my @ls_inputs_71 = ();
push @ls_inputs_71, 'test_file.txt';
push @ls_inputs_71, 'test_file_copy.txt';
push @ls_inputs_71, 'test_file_moved.txt';
my @ls_files_72 = ();
my @ls_dirs_73 = ();
my $ls_show_headers_74 = scalar(@ls_inputs_71) > 1;
for my $ls_item_75 (@ls_inputs_71) {
    if ( -f $ls_item_75 ) {
        push @ls_files_72, $ls_item_75;
    }
    elsif ( -d $ls_item_75 ) {
        push @ls_dirs_73, $ls_item_75;
    }
    else {
        $ls_all_found_70 = 0;
    }
}
@ls_files_72 = sort { $a cmp $b } @ls_files_72;
@ls_dirs_73 = sort { $a cmp $b } @ls_dirs_73;
if (@ls_files_72) {
    push @ls_files_69, join("\n", @ls_files_72);
}
for my $ls_dir_76 (@ls_dirs_73) {
    my @ls_dir_entries_77 = ();
    if ( opendir my $dh, $ls_dir_76 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_77, $file;
        }
        closedir $dh;
        @ls_dir_entries_77 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_77;
        if ( $ls_show_headers_74 ) {
            if ( @ls_dir_entries_77 ) {
                push @ls_files_69, $ls_dir_76 . ":\n" . join("\n", @ls_dir_entries_77);
            } else {
                push @ls_files_69, $ls_dir_76 . ':';
            }
        }
        elsif ( @ls_dir_entries_77 ) {
            push @ls_files_69, join("\n", @ls_dir_entries_77);
        }
    }
    else {
        $ls_all_found_70 = 0;
    }
}
if (@ls_files_69) {
    print join "\n\n", @ls_files_69;
    print "\n";
}
if ( $ls_all_found_70 ) {
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
my $mv_result;
$mv_result = do {
    my $left_result_78 = do {
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
        my $right_result_78 = do { ("Move successful") };
        $left_result_78 . $right_result_78;
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
my @ls_files_79 = ();
my $ls_all_found_80 = 1;
my @ls_inputs_81 = ();
push @ls_inputs_81, 'test_file.txt';
push @ls_inputs_81, 'test_file_copy.txt';
push @ls_inputs_81, 'test_file_moved.txt';
my @ls_files_82 = ();
my @ls_dirs_83 = ();
my $ls_show_headers_84 = scalar(@ls_inputs_81) > 1;
for my $ls_item_85 (@ls_inputs_81) {
    if ( -f $ls_item_85 ) {
        push @ls_files_82, $ls_item_85;
    }
    elsif ( -d $ls_item_85 ) {
        push @ls_dirs_83, $ls_item_85;
    }
    else {
        $ls_all_found_80 = 0;
    }
}
@ls_files_82 = sort { $a cmp $b } @ls_files_82;
@ls_dirs_83 = sort { $a cmp $b } @ls_dirs_83;
if (@ls_files_82) {
    push @ls_files_79, join("\n", @ls_files_82);
}
for my $ls_dir_86 (@ls_dirs_83) {
    my @ls_dir_entries_87 = ();
    if ( opendir my $dh, $ls_dir_86 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_87, $file;
        }
        closedir $dh;
        @ls_dir_entries_87 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_87;
        if ( $ls_show_headers_84 ) {
            if ( @ls_dir_entries_87 ) {
                push @ls_files_79, $ls_dir_86 . ":\n" . join("\n", @ls_dir_entries_87);
            } else {
                push @ls_files_79, $ls_dir_86 . ':';
            }
        }
        elsif ( @ls_dir_entries_87 ) {
            push @ls_files_79, join("\n", @ls_dir_entries_87);
        }
    }
    else {
        $ls_all_found_80 = 0;
    }
}
if (@ls_files_79) {
    print join "\n\n", @ls_files_79;
    print "\n";
}
if ( $ls_all_found_80 ) {
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
my $rm_result;
$rm_result = do {
    my $left_result_88 = do {
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
        my $right_result_88 = do { ("Remove successful") };
        $left_result_88 . $right_result_88;
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
my @ls_files_89 = ();
my $ls_all_found_90 = 1;
my @ls_inputs_91 = ();
push @ls_inputs_91, 'test_file.txt';
push @ls_inputs_91, 'test_file_copy.txt';
push @ls_inputs_91, 'test_file_moved.txt';
my @ls_files_92 = ();
my @ls_dirs_93 = ();
my $ls_show_headers_94 = scalar(@ls_inputs_91) > 1;
for my $ls_item_95 (@ls_inputs_91) {
    if ( -f $ls_item_95 ) {
        push @ls_files_92, $ls_item_95;
    }
    elsif ( -d $ls_item_95 ) {
        push @ls_dirs_93, $ls_item_95;
    }
    else {
        $ls_all_found_90 = 0;
    }
}
@ls_files_92 = sort { $a cmp $b } @ls_files_92;
@ls_dirs_93 = sort { $a cmp $b } @ls_dirs_93;
if (@ls_files_92) {
    push @ls_files_89, join("\n", @ls_files_92);
}
for my $ls_dir_96 (@ls_dirs_93) {
    my @ls_dir_entries_97 = ();
    if ( opendir my $dh, $ls_dir_96 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_97, $file;
        }
        closedir $dh;
        @ls_dir_entries_97 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_97;
        if ( $ls_show_headers_94 ) {
            if ( @ls_dir_entries_97 ) {
                push @ls_files_89, $ls_dir_96 . ":\n" . join("\n", @ls_dir_entries_97);
            } else {
                push @ls_files_89, $ls_dir_96 . ':';
            }
        }
        elsif ( @ls_dir_entries_97 ) {
            push @ls_files_89, join("\n", @ls_dir_entries_97);
        }
    }
    else {
        $ls_all_found_90 = 0;
    }
}
if (@ls_files_89) {
    print join "\n\n", @ls_files_89;
    print "\n";
}
if ( $ls_all_found_90 ) {
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
my $mkdir_result;
$mkdir_result = do {
    my $left_result_98 = do {
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
        my $right_result_98 = do { ("Directory created") };
        $left_result_98 . $right_result_98;
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
my @ls_files_100 = ();
my $ls_all_found_101 = 1;
my @ls_inputs_102 = ();
push @ls_inputs_102, 'test_dir';
my @ls_files_103 = ();
my @ls_dirs_104 = ();
my $ls_show_headers_105 = scalar(@ls_inputs_102) > 1;
for my $ls_item_106 (@ls_inputs_102) {
    if ( -f $ls_item_106 ) {
        push @ls_files_103, $ls_item_106;
    }
    elsif ( -d $ls_item_106 ) {
        push @ls_dirs_104, $ls_item_106;
    }
    else {
        $ls_all_found_101 = 0;
    }
}
@ls_files_103 = sort { $a cmp $b } @ls_files_103;
@ls_dirs_104 = sort { $a cmp $b } @ls_dirs_104;
if (@ls_files_103) {
    push @ls_files_100, join("\n", @ls_files_103);
}
for my $ls_dir_107 (@ls_dirs_104) {
    my @ls_dir_entries_108 = ();
    if ( opendir my $dh, $ls_dir_107 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_108, $file;
        }
        closedir $dh;
        @ls_dir_entries_108 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_108;
        if ( $ls_show_headers_105 ) {
            if ( @ls_dir_entries_108 ) {
                push @ls_files_100, $ls_dir_107 . ":\n" . join("\n", @ls_dir_entries_108);
            } else {
                push @ls_files_100, $ls_dir_107 . ':';
            }
        }
        elsif ( @ls_dir_entries_108 ) {
            push @ls_files_100, join("\n", @ls_dir_entries_108);
        }
    }
    else {
        $ls_all_found_101 = 0;
    }
}
if (@ls_files_100) {
    print join "\n", @ls_files_100;
    print "\n";
}
if ( $ls_all_found_101 ) {
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
my $touch_result;
$touch_result = do {
    my $left_result_109 = do {
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
        my $right_result_109 = do { ("File touched") };
        $left_result_109 . $right_result_109;
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
print "=== File Manipulation Commands Complete ===\n";

exit $main_exit_code;

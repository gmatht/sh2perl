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
    my $left_result_66 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Copy qw(copy);
            if ( -e 'test_file.txt' ) {
                if ( -d 'test_file_copy.txt' ) {
                    require File::Copy; File::Copy::copy('test_file.txt', 'test_file_copy.txt' . '/' . ('test_file.txt' =~ m|([^/]+)$|)[0]);
                } else {
                    require File::Copy; File::Copy::copy('test_file.txt', 'test_file_copy.txt');
                }
            } else {
                croak "cp: cannot stat 'test_file.txt': No such file or directory\n";
            }
            1;
            };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_66 = do { ("Copy successful") };
        $left_result_66 . $right_result_66;
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
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
    my @ls_files_67 = ();
    my $ls_all_found_68 = 1;
    my @ls_inputs_69 = ();
    push @ls_inputs_69, 'test_file.txt';
    push @ls_inputs_69, 'test_file_copy.txt';
    push @ls_inputs_69, 'test_file_moved.txt';
    my @ls_files_70 = ();
    my @ls_dirs_71 = ();
    my $ls_show_headers_72 = scalar(@ls_inputs_69) > 1;
    for my $ls_item_73 (@ls_inputs_69) {
        if ( -f $ls_item_73 ) {
            push @ls_files_70, $ls_item_73;
        }
        elsif ( -d $ls_item_73 ) {
            push @ls_dirs_71, $ls_item_73;
        }
        else {
            $ls_all_found_68 = 0;
        }
    }
    @ls_files_70 = sort { $a cmp $b } @ls_files_70;
    @ls_dirs_71 = sort { $a cmp $b } @ls_dirs_71;
    if (@ls_files_70) {
        push @ls_files_67, join("\n", @ls_files_70);
    }
    for my $ls_dir_74 (@ls_dirs_71) {
        my @ls_dir_entries_75 = ();
        if ( opendir my $dh, $ls_dir_74 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                push @ls_dir_entries_75, $file;
            }
            closedir $dh;
            @ls_dir_entries_75 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_75;
            if ( $ls_show_headers_72 ) {
                if ( @ls_dir_entries_75 ) {
                    push @ls_files_67, $ls_dir_74 . ":\n" . join("\n", @ls_dir_entries_75);
                } else {
                    push @ls_files_67, $ls_dir_74 . ':';
                }
            }
            elsif ( @ls_dir_entries_75 ) {
                push @ls_files_67, join("\n", @ls_dir_entries_75);
            }
        }
        else {
            $ls_all_found_68 = 0;
        }
    }
    if (@ls_files_67) {
        print join "\n\n", @ls_files_67;
        print "\n";
    }
    if ( $ls_all_found_68 ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
        $main_exit_code = $CHILD_ERROR;
    }
};
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
$main_exit_code = 0;
my $mv_result = do {
    my $left_result_76 = do {
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
        my $right_result_76 = do { ("Move successful") };
        $left_result_76 . $right_result_76;
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
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
    my @ls_files_77 = ();
    my $ls_all_found_78 = 1;
    my @ls_inputs_79 = ();
    push @ls_inputs_79, 'test_file.txt';
    push @ls_inputs_79, 'test_file_copy.txt';
    push @ls_inputs_79, 'test_file_moved.txt';
    my @ls_files_80 = ();
    my @ls_dirs_81 = ();
    my $ls_show_headers_82 = scalar(@ls_inputs_79) > 1;
    for my $ls_item_83 (@ls_inputs_79) {
        if ( -f $ls_item_83 ) {
            push @ls_files_80, $ls_item_83;
        }
        elsif ( -d $ls_item_83 ) {
            push @ls_dirs_81, $ls_item_83;
        }
        else {
            $ls_all_found_78 = 0;
        }
    }
    @ls_files_80 = sort { $a cmp $b } @ls_files_80;
    @ls_dirs_81 = sort { $a cmp $b } @ls_dirs_81;
    if (@ls_files_80) {
        push @ls_files_77, join("\n", @ls_files_80);
    }
    for my $ls_dir_84 (@ls_dirs_81) {
        my @ls_dir_entries_85 = ();
        if ( opendir my $dh, $ls_dir_84 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                push @ls_dir_entries_85, $file;
            }
            closedir $dh;
            @ls_dir_entries_85 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_85;
            if ( $ls_show_headers_82 ) {
                if ( @ls_dir_entries_85 ) {
                    push @ls_files_77, $ls_dir_84 . ":\n" . join("\n", @ls_dir_entries_85);
                } else {
                    push @ls_files_77, $ls_dir_84 . ':';
                }
            }
            elsif ( @ls_dir_entries_85 ) {
                push @ls_files_77, join("\n", @ls_dir_entries_85);
            }
        }
        else {
            $ls_all_found_78 = 0;
        }
    }
    if (@ls_files_77) {
        print join "\n\n", @ls_files_77;
        print "\n";
    }
    if ( $ls_all_found_78 ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
        $main_exit_code = $CHILD_ERROR;
    }
};
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
$main_exit_code = 0;
my $rm_result = do {
    my $left_result_86 = do {
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
        my $right_result_86 = do { ("Remove successful") };
        $left_result_86 . $right_result_86;
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
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
    my @ls_files_87 = ();
    my $ls_all_found_88 = 1;
    my @ls_inputs_89 = ();
    push @ls_inputs_89, 'test_file.txt';
    push @ls_inputs_89, 'test_file_copy.txt';
    push @ls_inputs_89, 'test_file_moved.txt';
    my @ls_files_90 = ();
    my @ls_dirs_91 = ();
    my $ls_show_headers_92 = scalar(@ls_inputs_89) > 1;
    for my $ls_item_93 (@ls_inputs_89) {
        if ( -f $ls_item_93 ) {
            push @ls_files_90, $ls_item_93;
        }
        elsif ( -d $ls_item_93 ) {
            push @ls_dirs_91, $ls_item_93;
        }
        else {
            $ls_all_found_88 = 0;
        }
    }
    @ls_files_90 = sort { $a cmp $b } @ls_files_90;
    @ls_dirs_91 = sort { $a cmp $b } @ls_dirs_91;
    if (@ls_files_90) {
        push @ls_files_87, join("\n", @ls_files_90);
    }
    for my $ls_dir_94 (@ls_dirs_91) {
        my @ls_dir_entries_95 = ();
        if ( opendir my $dh, $ls_dir_94 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                push @ls_dir_entries_95, $file;
            }
            closedir $dh;
            @ls_dir_entries_95 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_95;
            if ( $ls_show_headers_92 ) {
                if ( @ls_dir_entries_95 ) {
                    push @ls_files_87, $ls_dir_94 . ":\n" . join("\n", @ls_dir_entries_95);
                } else {
                    push @ls_files_87, $ls_dir_94 . ':';
                }
            }
            elsif ( @ls_dir_entries_95 ) {
                push @ls_files_87, join("\n", @ls_dir_entries_95);
            }
        }
        else {
            $ls_all_found_88 = 0;
        }
    }
    if (@ls_files_87) {
        print join "\n\n", @ls_files_87;
        print "\n";
    }
    if ( $ls_all_found_88 ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
        $main_exit_code = $CHILD_ERROR;
    }
};
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
$main_exit_code = 0;
my $mkdir_result = do {
    my $left_result_96 = do {
        $CHILD_ERROR = 0;
        my $eval_result = eval {
            use File::Path qw(make_path);
            if ( mkdir 'test_dir' ) {
                }
            else {
                croak "mkdir: cannot create directory " . 'test_dir' . ": File exists\n";
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
        my $right_result_96 = do { ("Directory created") };
        $left_result_96 . $right_result_96;
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
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
    my @ls_files_98 = ();
    my $ls_all_found_99 = 1;
    my @ls_inputs_100 = ();
    push @ls_inputs_100, 'test_dir';
    my @ls_files_101 = ();
    my @ls_dirs_102 = ();
    my $ls_show_headers_103 = scalar(@ls_inputs_100) > 1;
    for my $ls_item_104 (@ls_inputs_100) {
        if ( -f $ls_item_104 ) {
            push @ls_files_101, $ls_item_104;
        }
        elsif ( -d $ls_item_104 ) {
            push @ls_dirs_102, $ls_item_104;
        }
        else {
            $ls_all_found_99 = 0;
        }
    }
    @ls_files_101 = sort { $a cmp $b } @ls_files_101;
    @ls_dirs_102 = sort { $a cmp $b } @ls_dirs_102;
    if (@ls_files_101) {
        push @ls_files_98, join("\n", @ls_files_101);
    }
    for my $ls_dir_105 (@ls_dirs_102) {
        my @ls_dir_entries_106 = ();
        if ( opendir my $dh, $ls_dir_105 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                push @ls_dir_entries_106, $file;
            }
            closedir $dh;
            @ls_dir_entries_106 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_106;
            if ( $ls_show_headers_103 ) {
                if ( @ls_dir_entries_106 ) {
                    push @ls_files_98, $ls_dir_105 . ":\n" . join("\n", @ls_dir_entries_106);
                } else {
                    push @ls_files_98, $ls_dir_105 . ':';
                }
            }
            elsif ( @ls_dir_entries_106 ) {
                push @ls_files_98, join("\n", @ls_dir_entries_106);
            }
        }
        else {
            $ls_all_found_99 = 0;
        }
    }
    if (@ls_files_98) {
        print join "\n", @ls_files_98;
        print "\n";
    }
    if ( $ls_all_found_99 ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
        $main_exit_code = $CHILD_ERROR;
    }
};
if ( !defined $ls_success || $ls_success == 0 ) {
        print "Directory not found\n";
}
$main_exit_code = 0;
my $touch_result = do {
    my $left_result_107 = do {
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
        my $right_result_107 = do { ("File touched") };
        $left_result_107 . $right_result_107;
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
do {
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
};
if ($CHILD_ERROR != 0) {
    1;
}
print "=== File Manipulation Commands Complete ===\n";

exit $main_exit_code;

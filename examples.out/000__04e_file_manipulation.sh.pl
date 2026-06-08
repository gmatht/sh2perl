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
<<<<<<< HEAD
    my $left_result_68 = do {
        $CHILD_ERROR = 0;
=======
    my $left_result_67 = do {
        local $CHILD_ERROR = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp test_file.txt test_file_copy.txt';
                my $cp_output = qx{$cp_cmd};
                # print $cp_output;
                $cp_output;
            };
<<<<<<< HEAD
            1;
            };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
=======
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
<<<<<<< HEAD
        my $right_result_68 = do { ("Copy successful") };
        $left_result_68 . $right_result_68;
=======
        my $right_result_67 = do { ("Copy successful") };
        $left_result_67 . $right_result_67;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
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
=======
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
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
    my $left_result_78 = do {
        $CHILD_ERROR = 0;
=======
    my $left_result_77 = do {
        local $CHILD_ERROR = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
            1;
            };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
=======
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
<<<<<<< HEAD
        my $right_result_78 = do { ("Move successful") };
        $left_result_78 . $right_result_78;
=======
        my $right_result_77 = do { ("Move successful") };
        $left_result_77 . $right_result_77;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
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
=======
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
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
    my $left_result_88 = do {
        $CHILD_ERROR = 0;
=======
    my $left_result_87 = do {
        local $CHILD_ERROR = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        my $eval_result = eval {
            if ( -e "test_file.txt" ) {
                if ( -d "test_file.txt" ) {
                    croak "rm: ", "test_file.txt",
                      " is a directory (use -r to remove recursively)\n";
                }
                else {
                    if ( unlink "test_file.txt" ) {
<<<<<<< HEAD
                                }
=======
                        $main_exit_code = 0;
                    }
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
                                }
=======
                        $main_exit_code = 0;
                    }
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
            1;
            };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
=======
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
<<<<<<< HEAD
        my $right_result_88 = do { ("Remove successful") };
        $left_result_88 . $right_result_88;
=======
        my $right_result_87 = do { ("Remove successful") };
        $left_result_87 . $right_result_87;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
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
=======
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
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
    my $left_result_98 = do { my $mkdir_cmd = 'mkdir test_dir'; my $mkdir_output = qx{$mkdir_cmd}; $CHILD_ERROR = $? >> 8; $mkdir_output; };
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_98 = do { ("Directory created") };
        $left_result_98 . $right_result_98;
=======
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
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
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
=======
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
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
    my $left_result_109 = do {
        $CHILD_ERROR = 0;
=======
    my $left_result_108 = do {
        local $CHILD_ERROR = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
            $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            $CHILD_ERROR = 256;
=======
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        }
        q{};
};
    if ( $CHILD_ERROR == 0 ) {
<<<<<<< HEAD
        my $right_result_109 = do { ("File touched") };
        $left_result_109 . $right_result_109;
=======
        my $right_result_108 = do { ("File touched") };
        $left_result_108 . $right_result_108;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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

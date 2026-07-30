#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '000__04e_file_manipulation.sh';
print "=== File Manipulation Commands ===\n";
open my $fh, '>', 'test_file.txt' or die "test_file.txt: $!\n";
print {$fh}("test content", "\n");
close $fh;
my $cp_result = do { my $__cs = do {
    my $left_result_63 = do { my $__cs = do {
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
}; chomp $__cs; $__cs; };
    if ($CHILD_ERROR == 0) {
        my $right_result_63 = do { my $__cs = "Copy successful"; chomp $__cs; $__cs; };
        $left_result_63 . $right_result_63;
    } else {
        q{};
    }
}; chomp $__cs; $__cs; };
print "Copy result: ${cp_result}\n";
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
    my @ls_files_64 = ();
    my $ls_all_found_65 = 1;
    my @ls_inputs_66 = ();
    push @ls_inputs_66, 'test_file.txt';
    push @ls_inputs_66, 'test_file_copy.txt';
    push @ls_inputs_66, 'test_file_moved.txt';
    my @ls_files_67 = ();
    my @ls_dirs_68 = ();
    my $ls_show_headers_69 = scalar(@ls_inputs_66) > 1;
    for my $ls_item_70 (@ls_inputs_66) {
        if ( -f $ls_item_70 ) {
            push @ls_files_67, $ls_item_70;
        }
        elsif ( -d $ls_item_70 ) {
            push @ls_dirs_68, $ls_item_70;
        }
        else {
            $ls_all_found_65 = 0;
        }
    }
    @ls_files_67 = sort { $a cmp $b } @ls_files_67;
    @ls_dirs_68 = sort { $a cmp $b } @ls_dirs_68;
    if (@ls_files_67) {
        push @ls_files_64, join("\n", @ls_files_67);
    }
    for my $ls_dir_71 (@ls_dirs_68) {
        my @ls_dir_entries_72 = ();
        if ( opendir my $dh, $ls_dir_71 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
                push @ls_dir_entries_72, $file;
            }
            closedir $dh;
            @ls_dir_entries_72 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_dir_entries_72;
            if ( $ls_show_headers_69 ) {
                if ( @ls_dir_entries_72 ) {
                    push @ls_files_64, $ls_dir_71 . ":\n" . join("\n", @ls_dir_entries_72);
                } else {
                    push @ls_files_64, $ls_dir_71 . ':';
                }
            }
            elsif ( @ls_dir_entries_72 ) {
                push @ls_files_64, join("\n", @ls_dir_entries_72);
            }
        }
        else {
            $ls_all_found_65 = 0;
        }
    }
    if (@ls_files_64) {
        print join "\n\n", @ls_files_64;
        print "\n";
    }
    if ( $ls_all_found_65 ) {
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
my $mv_result = do { my $__cs = do {
    my $left_result_73 = do { my $__cs = do {
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
}; chomp $__cs; $__cs; };
    if ($CHILD_ERROR == 0) {
        my $right_result_73 = do { my $__cs = "Move successful"; chomp $__cs; $__cs; };
        $left_result_73 . $right_result_73;
    } else {
        q{};
    }
}; chomp $__cs; $__cs; };
print "Move result: ${mv_result}\n";
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
    my @ls_files_74 = ();
    my $ls_all_found_75 = 1;
    my @ls_inputs_76 = ();
    push @ls_inputs_76, 'test_file.txt';
    push @ls_inputs_76, 'test_file_copy.txt';
    push @ls_inputs_76, 'test_file_moved.txt';
    my @ls_files_77 = ();
    my @ls_dirs_78 = ();
    my $ls_show_headers_79 = scalar(@ls_inputs_76) > 1;
    for my $ls_item_80 (@ls_inputs_76) {
        if ( -f $ls_item_80 ) {
            push @ls_files_77, $ls_item_80;
        }
        elsif ( -d $ls_item_80 ) {
            push @ls_dirs_78, $ls_item_80;
        }
        else {
            $ls_all_found_75 = 0;
        }
    }
    @ls_files_77 = sort { $a cmp $b } @ls_files_77;
    @ls_dirs_78 = sort { $a cmp $b } @ls_dirs_78;
    if (@ls_files_77) {
        push @ls_files_74, join("\n", @ls_files_77);
    }
    for my $ls_dir_81 (@ls_dirs_78) {
        my @ls_dir_entries_82 = ();
        if ( opendir my $dh, $ls_dir_81 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
                push @ls_dir_entries_82, $file;
            }
            closedir $dh;
            @ls_dir_entries_82 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_dir_entries_82;
            if ( $ls_show_headers_79 ) {
                if ( @ls_dir_entries_82 ) {
                    push @ls_files_74, $ls_dir_81 . ":\n" . join("\n", @ls_dir_entries_82);
                } else {
                    push @ls_files_74, $ls_dir_81 . ':';
                }
            }
            elsif ( @ls_dir_entries_82 ) {
                push @ls_files_74, join("\n", @ls_dir_entries_82);
            }
        }
        else {
            $ls_all_found_75 = 0;
        }
    }
    if (@ls_files_74) {
        print join "\n\n", @ls_files_74;
        print "\n";
    }
    if ( $ls_all_found_75 ) {
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
my $rm_result = do { my $__cs = do {
    my $left_result_83 = do { my $__cs = do {
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
}; chomp $__cs; $__cs; };
    if ($CHILD_ERROR == 0) {
        my $right_result_83 = do { my $__cs = "Remove successful"; chomp $__cs; $__cs; };
        $left_result_83 . $right_result_83;
    } else {
        q{};
    }
}; chomp $__cs; $__cs; };
print "Remove result: ${rm_result}\n";
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
    my @ls_files_84 = ();
    my $ls_all_found_85 = 1;
    my @ls_inputs_86 = ();
    push @ls_inputs_86, 'test_file.txt';
    push @ls_inputs_86, 'test_file_copy.txt';
    push @ls_inputs_86, 'test_file_moved.txt';
    my @ls_files_87 = ();
    my @ls_dirs_88 = ();
    my $ls_show_headers_89 = scalar(@ls_inputs_86) > 1;
    for my $ls_item_90 (@ls_inputs_86) {
        if ( -f $ls_item_90 ) {
            push @ls_files_87, $ls_item_90;
        }
        elsif ( -d $ls_item_90 ) {
            push @ls_dirs_88, $ls_item_90;
        }
        else {
            $ls_all_found_85 = 0;
        }
    }
    @ls_files_87 = sort { $a cmp $b } @ls_files_87;
    @ls_dirs_88 = sort { $a cmp $b } @ls_dirs_88;
    if (@ls_files_87) {
        push @ls_files_84, join("\n", @ls_files_87);
    }
    for my $ls_dir_91 (@ls_dirs_88) {
        my @ls_dir_entries_92 = ();
        if ( opendir my $dh, $ls_dir_91 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
                push @ls_dir_entries_92, $file;
            }
            closedir $dh;
            @ls_dir_entries_92 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_dir_entries_92;
            if ( $ls_show_headers_89 ) {
                if ( @ls_dir_entries_92 ) {
                    push @ls_files_84, $ls_dir_91 . ":\n" . join("\n", @ls_dir_entries_92);
                } else {
                    push @ls_files_84, $ls_dir_91 . ':';
                }
            }
            elsif ( @ls_dir_entries_92 ) {
                push @ls_files_84, join("\n", @ls_dir_entries_92);
            }
        }
        else {
            $ls_all_found_85 = 0;
        }
    }
    if (@ls_files_84) {
        print join "\n\n", @ls_files_84;
        print "\n";
    }
    if ( $ls_all_found_85 ) {
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
my $mkdir_result = do { my $__cs = do {
    my $left_result_93 = do { my $__cs = do {
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
}; chomp $__cs; $__cs; };
    if ($CHILD_ERROR == 0) {
        my $right_result_93 = do { my $__cs = "Directory created"; chomp $__cs; $__cs; };
        $left_result_93 . $right_result_93;
    } else {
        q{};
    }
}; chomp $__cs; $__cs; };
print "Mkdir result: ${mkdir_result}\n";
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
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
    my @ls_files_95 = ();
    my $ls_all_found_96 = 1;
    my @ls_inputs_97 = ();
    push @ls_inputs_97, 'test_dir';
    my @ls_files_98 = ();
    my @ls_dirs_99 = ();
    my $ls_show_headers_100 = scalar(@ls_inputs_97) > 1;
    for my $ls_item_101 (@ls_inputs_97) {
        if ( -f $ls_item_101 ) {
            push @ls_files_98, $ls_item_101;
        }
        elsif ( -d $ls_item_101 ) {
            push @ls_dirs_99, $ls_item_101;
        }
        else {
            $ls_all_found_96 = 0;
        }
    }
    @ls_files_98 = sort { $a cmp $b } @ls_files_98;
    @ls_dirs_99 = sort { $a cmp $b } @ls_dirs_99;
    if (@ls_files_98) {
        push @ls_files_95, join("\n", @ls_files_98);
    }
    for my $ls_dir_102 (@ls_dirs_99) {
        my @ls_dir_entries_103 = ();
        if ( opendir my $dh, $ls_dir_102 ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
                push @ls_dir_entries_103, $file;
            }
            closedir $dh;
            @ls_dir_entries_103 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_dir_entries_103;
            if ( $ls_show_headers_100 ) {
                if ( @ls_dir_entries_103 ) {
                    push @ls_files_95, $ls_dir_102 . ":\n" . join("\n", @ls_dir_entries_103);
                } else {
                    push @ls_files_95, $ls_dir_102 . ':';
                }
            }
            elsif ( @ls_dir_entries_103 ) {
                push @ls_files_95, join("\n", @ls_dir_entries_103);
            }
        }
        else {
            $ls_all_found_96 = 0;
        }
    }
    if (@ls_files_95) {
        print join "\n", @ls_files_95;
        print "\n";
    }
    if ( $ls_all_found_96 ) {
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
my $touch_result = do { my $__cs = do {
    my $left_result_104 = do { my $__cs = do {
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
}; chomp $__cs; $__cs; };
    if ($CHILD_ERROR == 0) {
        my $right_result_104 = do { my $__cs = "File touched"; chomp $__cs; $__cs; };
        $left_result_104 . $right_result_104;
    } else {
        q{};
    }
}; chomp $__cs; $__cs; };
print "Touch result: ${touch_result}\n";
unlink('test_file.txt');
unlink('test_file_copy.txt');
unlink('test_file_moved.txt');
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
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
    0;
}
print "=== File Manipulation Commands Complete ===\n";

exit $main_exit_code;

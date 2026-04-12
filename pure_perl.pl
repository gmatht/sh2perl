```perl
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $main_exit_code = 0;

print "=== File Manipulation Commands ===\n";
{
    open my $original_stdout, '>&', STDOUT
    or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'test_file.txt'
    or croak "Cannot open file: $ERRNO";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
    or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $cp_result = do { my $cmd_result_3 = do {
    my $left_result_0 = do { my $cmd_result_1 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    my $err;
if (-e "test_file.txt") {
    my $dest = "test_file_copy.txt";
if (-d $dest) {
        $dest = "$dest/test_file.txt";
}
    if (copy("test_file.txt", $dest)) {
        # print "cp: copied test_file.txt to $dest\n";
} else {
        croak "cp: cannot copy test_file.txt to $dest: $ERRNO\n";
}
} else {
    croak "cp: test_file.txt: No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_1; $cmd_result_1; };
    if ($CHILD_ERROR == 0) {
        my $right_result_0 = do { my $cmd_result_2 = ("Copy successful"); chomp $cmd_result_2; $cmd_result_2; };
        $left_result_0 . $right_result_0;
    } else {
        $left_result_0;
    }
}; chomp $cmd_result_3; $cmd_result_3; };
print "Copy result: $cp_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_4 = ();
if (-f 'test_file.txt') {
    push @ls_files_4, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_4, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_4, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_4) {
    print join "\n", @ls_files_4, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mv_result = do { my $cmd_result_8 = do {
    my $left_result_5 = do { my $cmd_result_6 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    my $force = false;
if (-e "test_file_copy.txt") {
    my $dest = "test_file_moved.txt";
if (-e $dest && -d $dest) {
        $dest = "$dest/test_file_copy.txt";
}
    if (-e $dest && !$force) {
        croak "mv: $dest: File exists (use -f to force overwrite)\n";
}
    my $dest_dir = $dest;
$dest_dir =~ s/\/[^\/]*$//msx;
if ($dest_dir ne q{} && !-d $dest_dir) {
        make_path($dest_dir, {error => \$err});
if (@{$err}) {
            croak "mv: cannot create directory $dest_dir: $err->[0]\n";
}
    }
    if (move("test_file_copy.txt", $dest)) {
        # print "mv: moved test_file_copy.txt to $dest\n";
} else {
        croak "mv: cannot move test_file_copy.txt to $dest: $ERRNO\n";
}
} else {
    croak "mv: test_file_copy.txt: No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_6; $cmd_result_6; };
    if ($CHILD_ERROR == 0) {
        my $right_result_5 = do { my $cmd_result_7 = ("Move successful"); chomp $cmd_result_7; $cmd_result_7; };
        $left_result_5 . $right_result_5;
    } else {
        $left_result_5;
    }
}; chomp $cmd_result_8; $cmd_result_8; };
print "Move result: $mv_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_9 = ();
if (-f 'test_file.txt') {
    push @ls_files_9, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_9, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_9, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_9) {
    print join "\n", @ls_files_9, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $rm_result = do { my $cmd_result_13 = do {
    my $left_result_10 = do { my $cmd_result_11 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (-e "test_file.txt") {
if (-d "test_file.txt") {
croak "rm: ", "test_file.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file.txt") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_file.txt",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_file.txt",
    ": No such file or directory\n";
}
if (-e "test_file_moved.txt") {
if (-d "test_file_moved.txt") {
croak "rm: ", "test_file_moved.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_moved.txt") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_file_moved.txt",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_file_moved.txt",
    ": No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_11; $cmd_result_11; };
    if ($CHILD_ERROR == 0) {
        my $right_result_10 = do { my $cmd_result_12 = ("Remove successful"); chomp $cmd_result_12; $cmd_result_12; };
        $left_result_10 . $right_result_10;
    } else {
        $left_result_10;
    }
}; chomp $cmd_result_13; $cmd_result_13; };
print "Remove result: $rm_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_14 = ();
if (-f 'test_file.txt') {
    push @ls_files_14, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_14, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_14, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_14) {
    print join "\n", @ls_files_14, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mkdir_result = do { my $cmd_result_18 = do {
    my $left_result_15 = do { my $cmd_result_16 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (!-d "test_dir") {
if (mkdir "test_dir") {
# print "mkdir: created directory "test_dir"\n";
} else {
croak "mkdir: cannot create directory "test_dir": $ERRNO\n";
}
} else {
croak "mkdir: cannot create directory "test_dir": File exists\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_16; $cmd_result_16; };
    if ($CHILD_ERROR == 0) {
        my $right_result_15 = do { my $cmd_result_17 = ("Directory created"); chomp $cmd_result_17; $cmd_result_17; };
        $left_result_15 . $right_result_15;
    } else {
        $left_result_15;
    }
}; chomp $cmd_result_18; $cmd_result_18; };
print "Mkdir result: $mkdir_result\n";
if (-e "test_dir/file") {
my $current_time = time;
utime $current_time, $current_time, "test_dir/file";
} else {
if (open my $fh, '>', "test_dir/file") {
close $fh or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_dir/file", ": $ERRNO\n";
}
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_20 = ();
if (-f 'test_dir') {
    push @ls_files_20, 'test_dir';
} elsif (-d 'test_dir') {
    if (opendir my $dh, 'test_dir') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_20, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_20) {
    print join "\n", @ls_files_20, "\n";
}
if ($CHILD_ERROR != 0) {
        print "Directory not found\n";
}
my $touch_result = do { my $cmd_result_24 = do {
    my $left_result_21 = do { my $cmd_result_22 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (-e "test_file.txt") {
my $current_time = time;
utime $current_time, $current_time, "test_file.txt";
} else {
if (open my $fh, '>', "test_file.txt") {
close $fh or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_file.txt", ": $ERRNO\n";
}
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_22; $cmd_result_22; };
    if ($CHILD_ERROR == 0) {
        my $right_result_21 = do { my $cmd_result_23 = ("File touched"); chomp $cmd_result_23; $cmd_result_23; };
        $left_result_21 . $right_result_21;
    } else {
        $left_result_21;
    }
}; chomp $cmd_result_24; $cmd_result_24; };
print "Touch result: $touch_result\n";
if (-e "test_file.txt") {
if (-d "test_file.txt") {
carp "rm: carping: ", "test_file.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file.txt",
    ": No such file or directory\n";
}
if (-e "test_file_copy.txt") {
if (-d "test_file_copy.txt") {
carp "rm: carping: ", "test_file_copy.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_copy.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_copy.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_copy.txt",
    ": No such file or directory\n";
}
if (-e "test_file_moved.txt") {
if (-d "test_file_moved.txt") {
carp "rm: carping: ", "test_file_moved.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_moved.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_moved.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_moved.txt",
    ": No such file or directory\n";
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
if (-e "f") {
if (-d "f") {
my $err;
remove_tree("f", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "f", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "f") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "f",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "f",
    ": No such file or directory\n";
}
if (-e "test_dir") {
if (-d "test_dir") {
my $err;
remove_tree("test_dir", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "test_dir", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "test_dir") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_dir",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_dir",
    ": No such file or directory\n";
}
if ($CHILD_ERROR != 0) {
    system 'true';
}

exit $main_exit_code;

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $main_exit_code = 0;

print "=== File Manipulation Commands ===\n";
{
    open my $original_stdout, '>&', STDOUT
    or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'test_file.txt'
    or croak "Cannot open file: $ERRNO";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
    or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $cp_result = do { my $cmd_result_3 = do {
    my $left_result_0 = do { my $cmd_result_1 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    my $err;
if (-e "test_file.txt") {
    my $dest = "test_file_copy.txt";
if (-d $dest) {
        $dest = "$dest/test_file.txt";
}
    if (copy("test_file.txt", $dest)) {
        # print "cp: copied test_file.txt to $dest\n";
} else {
        croak "cp: cannot copy test_file.txt to $dest: $ERRNO\n";
}
} else {
    croak "cp: test_file.txt: No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_1; $cmd_result_1; };
    if ($CHILD_ERROR == 0) {
        my $right_result_0 = do { my $cmd_result_2 = ("Copy successful"); chomp $cmd_result_2; $cmd_result_2; };
        $left_result_0 . $right_result_0;
    } else {
        $left_result_0;
    }
}; chomp $cmd_result_3; $cmd_result_3; };
print "Copy result: $cp_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_4 = ();
if (-f 'test_file.txt') {
    push @ls_files_4, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_4, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_4, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_4) {
    print join "\n", @ls_files_4, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mv_result = do { my $cmd_result_8 = do {
    my $left_result_5 = do { my $cmd_result_6 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    my $force = false;
if (-e "test_file_copy.txt") {
    my $dest = "test_file_moved.txt";
if (-e $dest && -d $dest) {
        $dest = "$dest/test_file_copy.txt";
}
    if (-e $dest && !$force) {
        croak "mv: $dest: File exists (use -f to force overwrite)\n";
}
    my $dest_dir = $dest;
$dest_dir =~ s/\/[^\/]*$//msx;
if ($dest_dir ne q{} && !-d $dest_dir) {
        make_path($dest_dir, {error => \$err});
if (@{$err}) {
            croak "mv: cannot create directory $dest_dir: $err->[0]\n";
}
    }
    if (move("test_file_copy.txt", $dest)) {
        # print "mv: moved test_file_copy.txt to $dest\n";
} else {
        croak "mv: cannot move test_file_copy.txt to $dest: $ERRNO\n";
}
} else {
    croak "mv: test_file_copy.txt: No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_6; $cmd_result_6; };
    if ($CHILD_ERROR == 0) {
        my $right_result_5 = do { my $cmd_result_7 = ("Move successful"); chomp $cmd_result_7; $cmd_result_7; };
        $left_result_5 . $right_result_5;
    } else {
        $left_result_5;
    }
}; chomp $cmd_result_8; $cmd_result_8; };
print "Move result: $mv_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_9 = ();
if (-f 'test_file.txt') {
    push @ls_files_9, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_9, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_9, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_9) {
    print join "\n", @ls_files_9, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $rm_result = do { my $cmd_result_13 = do {
    my $left_result_10 = do { my $cmd_result_11 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (-e "test_file.txt") {
if (-d "test_file.txt") {
croak "rm: ", "test_file.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file.txt") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_file.txt",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_file.txt",
    ": No such file or directory\n";
}
if (-e "test_file_moved.txt") {
if (-d "test_file_moved.txt") {
croak "rm: ", "test_file_moved.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_moved.txt") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_file_moved.txt",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_file_moved.txt",
    ": No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_11; $cmd_result_11; };
    if ($CHILD_ERROR == 0) {
        my $right_result_10 = do { my $cmd_result_12 = ("Remove successful"); chomp $cmd_result_12; $cmd_result_12; };
        $left_result_10 . $right_result_10;
    } else {
        $left_result_10;
    }
}; chomp $cmd_result_13; $cmd_result_13; };
print "Remove result: $rm_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_14 = ();
if (-f 'test_file.txt') {
    push @ls_files_14, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_14, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_14, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_14) {
    print join "\n", @ls_files_14, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mkdir_result = do { my $cmd_result_18 = do {
    my $left_result_15 = do { my $cmd_result_16 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (!-d "test_dir") {
if (mkdir "test_dir") {
# print "mkdir: created directory "test_dir"\n";
} else {
croak "mkdir: cannot create directory "test_dir": $ERRNO\n";
}
} else {
croak "mkdir: cannot create directory "test_dir": File exists\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_16; $cmd_result_16; };
    if ($CHILD_ERROR == 0) {
        my $right_result_15 = do { my $cmd_result_17 = ("Directory created"); chomp $cmd_result_17; $cmd_result_17; };
        $left_result_15 . $right_result_15;
    } else {
        $left_result_15;
    }
}; chomp $cmd_result_18; $cmd_result_18; };
print "Mkdir result: $mkdir_result\n";
if (-e "test_dir/file") {
my $current_time = time;
utime $current_time, $current_time, "test_dir/file";
} else {
if (open my $fh, '>', "test_dir/file") {
close $fh or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_dir/file", ": $ERRNO\n";
}
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_20 = ();
if (-f 'test_dir') {
    push @ls_files_20, 'test_dir';
} elsif (-d 'test_dir') {
    if (opendir my $dh, 'test_dir') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_20, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_20) {
    print join "\n", @ls_files_20, "\n";
}
if ($CHILD_ERROR != 0) {
        print "Directory not found\n";
}
my $touch_result = do { my $cmd_result_24 = do {
    my $left_result_21 = do { my $cmd_result_22 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (-e "test_file.txt") {
my $current_time = time;
utime $current_time, $current_time, "test_file.txt";
} else {
if (open my $fh, '>', "test_file.txt") {
close $fh or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_file.txt", ": $ERRNO\n";
}
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_22; $cmd_result_22; };
    if ($CHILD_ERROR == 0) {
        my $right_result_21 = do { my $cmd_result_23 = ("File touched"); chomp $cmd_result_23; $cmd_result_23; };
        $left_result_21 . $right_result_21;
    } else {
        $left_result_21;
    }
}; chomp $cmd_result_24; $cmd_result_24; };
print "Touch result: $touch_result\n";
if (-e "test_file.txt") {
if (-d "test_file.txt") {
carp "rm: carping: ", "test_file.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file.txt",
    ": No such file or directory\n";
}
if (-e "test_file_copy.txt") {
if (-d "test_file_copy.txt") {
carp "rm: carping: ", "test_file_copy.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_copy.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_copy.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_copy.txt",
    ": No such file or directory\n";
}
if (-e "test_file_moved.txt") {
if (-d "test_file_moved.txt") {
carp "rm: carping: ", "test_file_moved.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_moved.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_moved.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_moved.txt",
    ": No such file or directory\n";
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
if (-e "f") {
if (-d "f") {
my $err;
remove_tree("f", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "f", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "f") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "f",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "f",
    ": No such file or directory\n";
}
if (-e "test_dir") {
if (-d "test_dir") {
my $err;
remove_tree("test_dir", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "test_dir", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "test_dir") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_dir",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_dir",
    ": No such file or directory\n";
}
if ($CHILD_ERROR != 0) {
    system 'true';
}

exit $main_exit_code;

```perl
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $main_exit_code = 0;

print "=== File Manipulation Commands ===\n";
{
    open my $original_stdout, '>&', STDOUT
    or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'test_file.txt'
    or croak "Cannot open file: $ERRNO";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
    or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $cp_result = do { my $cmd_result_3 = do {
    my $left_result_0 = do { my $cmd_result_1 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    my $err;
if (-e "test_file.txt") {
    my $dest = "test_file_copy.txt";
if (-d $dest) {
        $dest = "$dest/test_file.txt";
}
    if (copy("test_file.txt", $dest)) {
        # print "cp: copied test_file.txt to $dest\n";
} else {
        croak "cp: cannot copy test_file.txt to $dest: $ERRNO\n";
}
} else {
    croak "cp: test_file.txt: No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_1; $cmd_result_1; };
    if ($CHILD_ERROR == 0) {
        my $right_result_0 = do { my $cmd_result_2 = ("Copy successful"); chomp $cmd_result_2; $cmd_result_2; };
        $left_result_0 . $right_result_0;
    } else {
        $left_result_0;
    }
}; chomp $cmd_result_3; $cmd_result_3; };
print "Copy result: $cp_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_4 = ();
if (-f 'test_file.txt') {
    push @ls_files_4, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_4, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_4, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_4, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_4) {
    print join "\n", @ls_files_4, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mv_result = do { my $cmd_result_8 = do {
    my $left_result_5 = do { my $cmd_result_6 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    my $force = false;
if (-e "test_file_copy.txt") {
    my $dest = "test_file_moved.txt";
if (-e $dest && -d $dest) {
        $dest = "$dest/test_file_copy.txt";
}
    if (-e $dest && !$force) {
        croak "mv: $dest: File exists (use -f to force overwrite)\n";
}
    my $dest_dir = $dest;
$dest_dir =~ s/\/[^\/]*$//msx;
if ($dest_dir ne q{} && !-d $dest_dir) {
        make_path($dest_dir, {error => \$err});
if (@{$err}) {
            croak "mv: cannot create directory $dest_dir: $err->[0]\n";
}
    }
    if (move("test_file_copy.txt", $dest)) {
        # print "mv: moved test_file_copy.txt to $dest\n";
} else {
        croak "mv: cannot move test_file_copy.txt to $dest: $ERRNO\n";
}
} else {
    croak "mv: test_file_copy.txt: No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_6; $cmd_result_6; };
    if ($CHILD_ERROR == 0) {
        my $right_result_5 = do { my $cmd_result_7 = ("Move successful"); chomp $cmd_result_7; $cmd_result_7; };
        $left_result_5 . $right_result_5;
    } else {
        $left_result_5;
    }
}; chomp $cmd_result_8; $cmd_result_8; };
print "Move result: $mv_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_9 = ();
if (-f 'test_file.txt') {
    push @ls_files_9, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_9, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_9, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_9, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_9) {
    print join "\n", @ls_files_9, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $rm_result = do { my $cmd_result_13 = do {
    my $left_result_10 = do { my $cmd_result_11 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (-e "test_file.txt") {
if (-d "test_file.txt") {
croak "rm: ", "test_file.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file.txt") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_file.txt",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_file.txt",
    ": No such file or directory\n";
}
if (-e "test_file_moved.txt") {
if (-d "test_file_moved.txt") {
croak "rm: ", "test_file_moved.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_moved.txt") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_file_moved.txt",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_file_moved.txt",
    ": No such file or directory\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_11; $cmd_result_11; };
    if ($CHILD_ERROR == 0) {
        my $right_result_10 = do { my $cmd_result_12 = ("Remove successful"); chomp $cmd_result_12; $cmd_result_12; };
        $left_result_10 . $right_result_10;
    } else {
        $left_result_10;
    }
}; chomp $cmd_result_13; $cmd_result_13; };
print "Remove result: $rm_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_14 = ();
if (-f 'test_file.txt') {
    push @ls_files_14, 'test_file.txt';
} elsif (-d 'test_file.txt') {
    if (opendir my $dh, 'test_file.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_copy.txt') {
    push @ls_files_14, 'test_file_copy.txt';
} elsif (-d 'test_file_copy.txt') {
    if (opendir my $dh, 'test_file_copy.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (-f 'test_file_moved.txt') {
    push @ls_files_14, 'test_file_moved.txt';
} elsif (-d 'test_file_moved.txt') {
    if (opendir my $dh, 'test_file_moved.txt') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_14, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_14) {
    print join "\n", @ls_files_14, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mkdir_result = do { my $cmd_result_18 = do {
    my $left_result_15 = do { my $cmd_result_16 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (!-d "test_dir") {
if (mkdir "test_dir") {
# print "mkdir: created directory "test_dir"\n";
} else {
croak "mkdir: cannot create directory "test_dir": $ERRNO\n";
}
} else {
croak "mkdir: cannot create directory "test_dir": File exists\n";
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_16; $cmd_result_16; };
    if ($CHILD_ERROR == 0) {
        my $right_result_15 = do { my $cmd_result_17 = ("Directory created"); chomp $cmd_result_17; $cmd_result_17; };
        $left_result_15 . $right_result_15;
    } else {
        $left_result_15;
    }
}; chomp $cmd_result_18; $cmd_result_18; };
print "Mkdir result: $mkdir_result\n";
if (-e "test_dir/file") {
my $current_time = time;
utime $current_time, $current_time, "test_dir/file";
} else {
if (open my $fh, '>', "test_dir/file") {
close $fh or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_dir/file", ": $ERRNO\n";
}
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_20 = ();
if (-f 'test_dir') {
    push @ls_files_20, 'test_dir';
} elsif (-d 'test_dir') {
    if (opendir my $dh, 'test_dir') {
        while (my $file = readdir $dh) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_files_20, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_20) {
    print join "\n", @ls_files_20, "\n";
}
if ($CHILD_ERROR != 0) {
        print "Directory not found\n";
}
my $touch_result = do { my $cmd_result_24 = do {
    my $left_result_21 = do { my $cmd_result_22 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (-e "test_file.txt") {
my $current_time = time;
utime $current_time, $current_time, "test_file.txt";
} else {
if (open my $fh, '>', "test_file.txt") {
close $fh or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_file.txt", ": $ERRNO\n";
}
};
            local $CHILD_ERROR = 0;
            1;
    };
    if (!$eval_result) {
        local $CHILD_ERROR = 256;
    };
    q{};
}; chomp $cmd_result_22; $cmd_result_22; };
    if ($CHILD_ERROR == 0) {
        my $right_result_21 = do { my $cmd_result_23 = ("File touched"); chomp $cmd_result_23; $cmd_result_23; };
        $left_result_21 . $right_result_21;
    } else {
        $left_result_21;
    }
}; chomp $cmd_result_24; $cmd_result_24; };
print "Touch result: $touch_result\n";
if (-e "test_file.txt") {
if (-d "test_file.txt") {
carp "rm: carping: ", "test_file.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file.txt",
    ": No such file or directory\n";
}
if (-e "test_file_copy.txt") {
if (-d "test_file_copy.txt") {
carp "rm: carping: ", "test_file_copy.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_copy.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_copy.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_copy.txt",
    ": No such file or directory\n";
}
if (-e "test_file_moved.txt") {
if (-d "test_file_moved.txt") {
carp "rm: carping: ", "test_file_moved.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_moved.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_moved.txt",
    ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_moved.txt",
    ": No such file or directory\n";
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
if (-e "f") {
if (-d "f") {
my $err;
remove_tree("f", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "f", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "f") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "f",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "f",
    ": No such file or directory\n";
}
if (-e "test_dir") {
if (-d "test_dir") {
my $err;
remove_tree("test_dir", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "test_dir", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "test_dir") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_dir",
    ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_dir",
    ": No such file or directory\n";
}
if ($CHILD_ERROR != 0) {
    system 'true';
}

exit $main_exit_code;


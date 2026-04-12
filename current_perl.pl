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
--
Generated Perl code:
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
--
Generated Perl code:
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

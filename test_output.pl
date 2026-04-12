Running shell script: examples/000__03_file_manipulation_commands.sh
Generated Perl code:
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
use POSIX      qw(time);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "=== File Manipulation Commands ===\n";
print "=== cp command ===\n";
print "\n";
{
    open my $original_stdout, '>&', STDOUT
      or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'test_file.txt'
      or croak "Cannot open file: $ERRNO";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
      or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout
      or croak "Close failed: $ERRNO";
}
my $cp_result = do {
    my $left_result_0 = do {
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
        my $right_result_0 = ("Copy successful");
        $left_result_0 . $right_result_0;
    }
    else {
        q{};
    }
};
do {
    my $output = "Copy result: $cp_result";
    print $output;
    if ( !( $output =~ m{\n$}msx ) ) {
        print "\n";
    }
};
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_1 = ();
if ( -f 'test_file.txt' ) {
    push @ls_files_1, 'test_file.txt';
}
elsif ( -d 'test_file.txt' ) {
    if ( opendir my $dh, 'test_file.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_1, $file;
        }
        closedir $dh;
    }
}
if ( -f 'test_file_copy.txt' ) {
    push @ls_files_1, 'test_file_copy.txt';
}
elsif ( -d 'test_file_copy.txt' ) {
    if ( opendir my $dh, 'test_file_copy.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_1, $file;
        }
        closedir $dh;
    }
}
if ( -f 'test_file_moved.txt' ) {
    push @ls_files_1, 'test_file_moved.txt';
}
elsif ( -d 'test_file_moved.txt' ) {
    if ( opendir my $dh, 'test_file_moved.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_1, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_1) {
    print join "\n", @ls_files_1;
    print "\n";
    my $expected_count = 3;
    if ( @ls_files_1 == $expected_count ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
    }
}
else {
    local $CHILD_ERROR = 1;
    $ls_success = 0;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
print "\n";
print "=== mv command ===\n";
my $mv_result = do {
    my $left_result_2 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    my $force = 0;
if (-e 'test_file_copy.txt') {
    my $dest = 'test_file_moved.txt';
    if (-e $dest && -d $dest) {
        $dest = "$dest/'test_file_copy.txt'";
    }
    if (-e $dest && !$force) {
        croak "mv: $dest: File exists (use -f to force overwrite)\n";
    }
    my $dest_dir = $dest;
    $dest_dir =~ s/\/[^\/]*$//msx;
    if ($dest_dir eq $dest) {
        $dest_dir = q{};
    }
    if ($dest_dir ne q{} && !-d $dest_dir) {
        my $err;
        make_path($dest_dir, {error => \$err});
        if (@{$err}) {
            croak "mv: cannot create directory $dest_dir: $err->[0]\n";
        }
    }
    if (move('test_file_copy.txt', $dest)) {
        # print "mv: moved 'test_file_copy.txt' to $dest\n";
    } else {
        croak "mv: cannot move 'test_file_copy.txt' to $dest: $ERRNO\n";
    }
} else {
    croak "mv: 'test_file_copy.txt': No such file or directory\n";
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
        my $right_result_2 = ("Move successful");
        $left_result_2 . $right_result_2;
    }
    else {
        q{};
    }
};
do {
    my $output = "Move result: $mv_result";
    print $output;
    if ( !( $output =~ m{\n$}msx ) ) {
        print "\n";
    }
};
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_3 = ();
if ( -f 'test_file.txt' ) {
    push @ls_files_3, 'test_file.txt';
}
elsif ( -d 'test_file.txt' ) {
    if ( opendir my $dh, 'test_file.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_3, $file;
        }
        closedir $dh;
    }
}
if ( -f 'test_file_copy.txt' ) {
    push @ls_files_3, 'test_file_copy.txt';
}
elsif ( -d 'test_file_copy.txt' ) {
    if ( opendir my $dh, 'test_file_copy.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_3, $file;
        }
        closedir $dh;
    }
}
if ( -f 'test_file_moved.txt' ) {
    push @ls_files_3, 'test_file_moved.txt';
}
elsif ( -d 'test_file_moved.txt' ) {
    if ( opendir my $dh, 'test_file_moved.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_3, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_3) {
    print join "\n", @ls_files_3;
    print "\n";
    my $expected_count = 3;
    if ( @ls_files_3 == $expected_count ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
    }
}
else {
    local $CHILD_ERROR = 1;
    $ls_success = 0;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
print "\n";
print "=== rm command ===\n";
my $rm_result = do {
    my $left_result_4 = do {
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
        my $right_result_4 = ("Remove successful");
        $left_result_4 . $right_result_4;
    }
    else {
        q{};
    }
};
do {
    my $output = "Remove result: $rm_result";
    print $output;
    if ( !( $output =~ m{\n$}msx ) ) {
        print "\n";
    }
};
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_5 = ();
if ( -f 'test_file.txt' ) {
    push @ls_files_5, 'test_file.txt';
}
elsif ( -d 'test_file.txt' ) {
    if ( opendir my $dh, 'test_file.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_5, $file;
        }
        closedir $dh;
    }
}
if ( -f 'test_file_copy.txt' ) {
    push @ls_files_5, 'test_file_copy.txt';
}
elsif ( -d 'test_file_copy.txt' ) {
    if ( opendir my $dh, 'test_file_copy.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_5, $file;
        }
        closedir $dh;
    }
}
if ( -f 'test_file_moved.txt' ) {
    push @ls_files_5, 'test_file_moved.txt';
}
elsif ( -d 'test_file_moved.txt' ) {
    if ( opendir my $dh, 'test_file_moved.txt' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_5, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_5) {
    print join "\n", @ls_files_5;
    print "\n";
    my $expected_count = 3;
    if ( @ls_files_5 == $expected_count ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
    }
}
else {
    local $CHILD_ERROR = 1;
    $ls_success = 0;
}
if ( !defined $ls_success || $ls_success == 0 ) {
        print "No test files found\n";
}
print "\n";
print "=== mkdir command ===\n";
my $mkdir_result = do {
    my $left_result_6 = do {
    use File::Path qw(make_path);
    if (!-d 'test_dir') {
    if (mkdir 'test_dir') {
    # print "mkdir: created directory 'test_dir'\n";
    } else {
    croak "mkdir: cannot create directory 'test_dir': $ERRNO\n";
    }
    } else {
    print {\*STDERR} "mkdir: cannot create directory 'test_dir': File exists\n";
    local $CHILD_ERROR = 256;
    };
    q{};
    };
    if ( $CHILD_ERROR == 0 ) {
        my $right_result_6 = ("Directory created");
        $left_result_6 . $right_result_6;
    }
    else {
        q{};
    }
};
do {
    my $output = "Mkdir result: $mkdir_result";
    print $output;
    if ( !( $output =~ m{\n$}msx ) ) {
        print "\n";
    }
};
if (-e "test_dir/file") {
my $current_time = time;
utime $current_time, $current_time, "test_dir/file";
} else {
if (open my $fh, '>', "test_dir/file") {
close $fh
  or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_dir/file", ": $ERRNO\n";
}
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_8 = ();
if ( -f 'test_dir' ) {
    push @ls_files_8, 'test_dir';
}
elsif ( -d 'test_dir' ) {
    if ( opendir my $dh, 'test_dir' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_8, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_8) {
    print join "\n", @ls_files_8;
    print "\n";
    my $expected_count = 1;
    if ( @ls_files_8 == $expected_count ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
    }
}
else {
    local $CHILD_ERROR = 1;
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
    local $CHILD_ERROR = 1;
    croak "rm: ", "test_dir/file", ": No such file or directory\n";
}
if (-d 'test_dir') {
if (rmdir 'test_dir') {
} else {
croak "rmdir: cannot remove directory 'test_dir': $ERRNO\n";
}
} else {
croak "rmdir: 'test_dir': No such file or directory\n";
}
print "\n";
print "=== touch command ===\n";
my $touch_result = do {
    my $left_result_9 = do {
    local $CHILD_ERROR = 0;
    my $eval_result = eval {
    if (-e "test_file.txt") {
my $current_time = time;
utime $current_time, $current_time, "test_file.txt";
} else {
if (open my $fh, '>', "test_file.txt") {
close $fh
  or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_file.txt", ": $ERRNO\n";
}
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
        my $right_result_9 = ("File touched");
        $left_result_9 . $right_result_9;
    }
    else {
        q{};
    }
};
do {
    my $output = "Touch result: $touch_result";
    print $output;
    if ( !( $output =~ m{\n$}msx ) ) {
        print "\n";
    }
};
print "\n";
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

exit $main_exit_code;


--- Running generated Perl code ---
Exit code: exit code: 2

==================================================
TIMING COMPARISON
==================================================
Perl execution time:  0.1495 seconds
Bash execution time:  1.6097 seconds
Perl is 10.76x faster than Bash

==================================================
OUTPUT COMPARISON
==================================================
✗ DIFFERENCES FOUND:

STDOUT DIFFERENCES:
--- bash_stdout
+++ perl_stdout
-=== File Manipulation Commands ===
-=== cp command ===
-
-Copy result: Copy successful
-test_file.txt
-test_file_copy.txt
-No test files found
-
-=== mv command ===
-Move result: Move successful
-test_file.txt
-test_file_moved.txt
-No test files found
-
-=== rm command ===
-Remove result: Remove successful
-No test files found
-
-=== mkdir command ===
-Mkdir result: Directory created
-file
-
-=== touch command ===
-Touch result: File touched
-


STDERR DIFFERENCES:
--- bash_stderr
+++ perl_stderr
+Can't open perl script "__tmp_run.pl": No such file or directory


EXIT CODE DIFFERENCES:
Bash exit code: Some(0)
Perl exit code: Some(2)

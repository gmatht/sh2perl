#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use File::Basename;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = '070_cmp_basic.sh';
my $MAGIC_6  = 6;
my $MAGIC_5  = 5;
my $MAGIC_3  = 3;
my $MAGIC_10 = 10;

do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/cmp_a.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "abcdefghij\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/cmp_same.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "abcdefghij\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/cmp_diff.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "abcdeZghij\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/cmp_diff2.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "xyzdefghij\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/cmp_short.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "abc\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
if ( -e "/tmp/cmp_empty.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "/tmp/cmp_empty.txt";
}
else {
    if ( open my $fh, '>', "/tmp/cmp_empty.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "/tmp/cmp_empty.txt",
          ": $ERRNO\n";
    }
}
$main_exit_code = system('cmp', '/tmp/cmp_a.txt', '/tmp/cmp_same.txt') >> 8;
do {
    my $output = "exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '/tmp/cmp_a.txt', '/tmp/cmp_empty.txt') >> 8;
do {
    my $output = "exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '/tmp/cmp_empty.txt', '/tmp/cmp_empty.txt') >> 8;
do {
    my $output = "exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-s', '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-s exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-s', '/tmp/cmp_a.txt', '/tmp/cmp_same.txt') >> 8;
do {
    my $output = "-s same exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-l', '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-l exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-b', '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-b exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-n', q{5}, '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-n 5 exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-n', '10', '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-n 10 exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-n', '10', '/tmp/cmp_a.txt', '/tmp/cmp_short.txt') >> 8;
do {
    my $output = "-n 10 short exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-i', q{6}, '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-i 6 exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-i', q{3}, '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-i 3 exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-i', '0:6', '/tmp/cmp_a.txt', '/tmp/cmp_diff.txt') >> 8;
do {
    my $output = "-i 0:6 exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$main_exit_code = system('cmp', '-i', '5:0', '/tmp/cmp_a.txt', '/tmp/cmp_diff2.txt') >> 8;
do {
    my $output = "-i 5:0 exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'echo a');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_1} $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
my $output_ps_fh_2;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'echo a');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_2 = dirname($temp_file_ps_fh_2);
if (!-d $temp_dir_fh_2) { make_path($temp_dir_fh_2); }
open my $fh_ps_fh_2, '>', $temp_file_ps_fh_2 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_2} $output_ps_fh_2;
close $fh_ps_fh_2 or croak "Close failed: $ERRNO\n";
$main_exit_code = system('cmp', '-s', $temp_file_ps_fh_1, $temp_file_ps_fh_2) >> 8;
do {
    my $output = "aa -s exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'echo b');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_3 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_3 = dirname($temp_file_ps_fh_3);
if (!-d $temp_dir_fh_3) { make_path($temp_dir_fh_3); }
open my $fh_ps_fh_3, '>', $temp_file_ps_fh_3 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_3} $output_ps_fh_3;
close $fh_ps_fh_3 or croak "Close failed: $ERRNO\n";
my $temp_file_ps_fh_4 = q{/tmp} . '/process_sub_fh_4.tmp';
my $output_ps_fh_4;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'echo c');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_4 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_4 = dirname($temp_file_ps_fh_4);
if (!-d $temp_dir_fh_4) { make_path($temp_dir_fh_4); }
open my $fh_ps_fh_4, '>', $temp_file_ps_fh_4 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_4} $output_ps_fh_4;
close $fh_ps_fh_4 or croak "Close failed: $ERRNO\n";
$main_exit_code = system('cmp', '-s', $temp_file_ps_fh_3, $temp_file_ps_fh_4) >> 8;
do {
    my $output = "bc -s exit: ${\($? >> 8)}";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
if ( -e "/tmp/cmp_a.txt" ) {
    if ( -d "/tmp/cmp_a.txt" ) {
        carp "rm: carping: ", "/tmp/cmp_a.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "/tmp/cmp_a.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "/tmp/cmp_a.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "/tmp/cmp_same.txt" ) {
    if ( -d "/tmp/cmp_same.txt" ) {
        carp "rm: carping: ", "/tmp/cmp_same.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "/tmp/cmp_same.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "/tmp/cmp_same.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "/tmp/cmp_diff.txt" ) {
    if ( -d "/tmp/cmp_diff.txt" ) {
        carp "rm: carping: ", "/tmp/cmp_diff.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "/tmp/cmp_diff.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "/tmp/cmp_diff.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "/tmp/cmp_diff2.txt" ) {
    if ( -d "/tmp/cmp_diff2.txt" ) {
        carp "rm: carping: ", "/tmp/cmp_diff2.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "/tmp/cmp_diff2.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "/tmp/cmp_diff2.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "/tmp/cmp_short.txt" ) {
    if ( -d "/tmp/cmp_short.txt" ) {
        carp "rm: carping: ", "/tmp/cmp_short.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "/tmp/cmp_short.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "/tmp/cmp_short.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "/tmp/cmp_empty.txt" ) {
    if ( -d "/tmp/cmp_empty.txt" ) {
        carp "rm: carping: ", "/tmp/cmp_empty.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "/tmp/cmp_empty.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "/tmp/cmp_empty.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

exit $main_exit_code;

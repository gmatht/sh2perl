#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== Here-string with grep -o ==\n";
my $here_string_content_fh_1 = "some pattern here";
my $grep_result_0;
my @grep_lines_0 = split /\n/msx, $here_string_content_fh_1;
my @grep_filtered_0 = grep { /pattern/msx } @grep_lines_0;
my @grep_matches_0;
foreach my $line (@grep_filtered_0) {
    if ($line =~ /(pattern)/msx) {
        push @grep_matches_0, $1;
    }
}
$grep_result_0 = join "\n", @grep_matches_0;
print $grep_result_0;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
print "== Process substitution with comm ==\n";
my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
my $output_ps_fh_2;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', q{printf 'a\nb\n'});
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
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', q{printf 'b\nc\n'});
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
my @file1_lines;
my @file2_lines;
if (open(my $fh1, '<', $temp_file_ps_fh_2)) {
    while (my $line = <$fh1>) {
        chomp $line;
        push @file1_lines, $line;
    }
    close($fh1);
}
if (open(my $fh2, '<', $temp_file_ps_fh_3)) {
    while (my $line = <$fh2>) {
        chomp $line;
        push @file2_lines, $line;
    }
    close($fh2);
}
my %file1_set = map { $_ => 1 } @file1_lines;
my %file2_set = map { $_ => 1 } @file2_lines;
my @common_lines;
foreach my $line (@file1_lines) {
    if (exists($file2_set{$line})) {
        push @common_lines, $line;
    }
}
my $result = "";
$result .= join("\n", @common_lines) . "\n";
chomp $result;
print $result;
print "\n";
print "== readarray/mapfile ==\n";
my $temp_file_ps_fh_4 = q{/tmp} . '/process_sub_fh_4.tmp';
my $output_ps_fh_4;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', q{printf 'x\ny\n'});
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
my @lines = ();
if (open(my $mapfile_fh, '<', $temp_file_ps_fh_4)) {
    while (my $line = <$mapfile_fh>) {
        chomp $line;
        push @lines, $line;
    }
    close($mapfile_fh);
}
foreach my $item (@lines) {
    printf('%s ', $item);
}
print "\n";
$CHILD_ERROR = 0;
print "== More process substitution examples ==\n";
my $temp_file_ps_fh_5 = q{/tmp} . '/process_sub_fh_5.tmp';
my $output_ps_fh_5;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_5 or croak "Cannot redirect STDOUT";
    my $output_169 = q{};
    my $output_printed_169;
    {
        my $pipeline_success_169 = 1;
        $output_169 .= "a\nc\nb";
    if ( !($output_169 =~ m{\n\z}msx) ) { $output_169 .= "\n"; }
    $CHILD_ERROR = 0;
            my @sort_lines_169_1 = split /\n/msx, $output_169;
        my @sort_sorted_169_1 = sort @sort_lines_169_1;
        my $output_169_1 = join "\n", @sort_sorted_169_1;
        if ($output_169_1 ne q{} && !($output_169_1 =~ m{\n\z}msx)) {
        $output_169_1 .= "\n";
        }
        $output_169 = $output_169_1;
        $output_169 = $output_169_1;
        if ($output_169 ne q{} && !defined $output_printed_169) {
            print $output_169;
            if (!($output_169 =~ m{\n\z}msx)) {
                print "\n";
            }
        }
        if ( !$pipeline_success_169 ) { $main_exit_code = 1; }
        }
}
use File::Path qw(make_path);
my $temp_dir_fh_5 = dirname($temp_file_ps_fh_5);
if (!-d $temp_dir_fh_5) { make_path($temp_dir_fh_5); }
open my $fh_ps_fh_5, '>', $temp_file_ps_fh_5 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_5} $output_ps_fh_5;
close $fh_ps_fh_5 or croak "Close failed: $ERRNO\n";
my $temp_file_ps_fh_6 = q{/tmp} . '/process_sub_fh_6.tmp';
my $output_ps_fh_6;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_6 or croak "Cannot redirect STDOUT";
    my $output_170 = q{};
    my $output_printed_170;
    {
        my $pipeline_success_170 = 1;
        $output_170 .= "a\nb\nd";
    if ( !($output_170 =~ m{\n\z}msx) ) { $output_170 .= "\n"; }
    $CHILD_ERROR = 0;
            my @sort_lines_170_1 = split /\n/msx, $output_170;
        my @sort_sorted_170_1 = sort @sort_lines_170_1;
        my $output_170_1 = join "\n", @sort_sorted_170_1;
        if ($output_170_1 ne q{} && !($output_170_1 =~ m{\n\z}msx)) {
        $output_170_1 .= "\n";
        }
        $output_170 = $output_170_1;
        $output_170 = $output_170_1;
        if ($output_170 ne q{} && !defined $output_printed_170) {
            print $output_170;
            if (!($output_170 =~ m{\n\z}msx)) {
                print "\n";
            }
        }
        if ( !$pipeline_success_170 ) { $main_exit_code = 1; }
        }
}
use File::Path qw(make_path);
my $temp_dir_fh_6 = dirname($temp_file_ps_fh_6);
if (!-d $temp_dir_fh_6) { make_path($temp_dir_fh_6); }
open my $fh_ps_fh_6, '>', $temp_file_ps_fh_6 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_6} $output_ps_fh_6;
close $fh_ps_fh_6 or croak "Close failed: $ERRNO\n";
$ENV{DIFF_TEMP_FILE1} = q{/tmp} . '/process_sub_fh_5.tmp';
$ENV{DIFF_TEMP_FILE2} = q{/tmp} . '/process_sub_fh_6.tmp';
my $diff_exit_code = 0;
my $diff_output = q{};
{
    my $diff_cmd = 'diff';
    my @diff_args = ($temp_file_ps_fh_5, $temp_file_ps_fh_6);
    my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
    if ($diff_pid) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $diff_output = <$diff_fh>;
        close $diff_fh;
        $diff_exit_code = $? >> 8;
    } else {
        carp "Cannot execute diff command: $OS_ERROR";
        $diff_output = q{};
        $diff_exit_code = 1;
    }
}
print $diff_output;
if ($diff_exit_code != 0) {
        print "Files differ\n";
}
my $temp_file_ps_fh_7 = q{/tmp} . '/process_sub_fh_7.tmp';
my $output_ps_fh_7;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'echo -e "name1\\nname2"');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_7 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_7 = dirname($temp_file_ps_fh_7);
if (!-d $temp_dir_fh_7) { make_path($temp_dir_fh_7); }
open my $fh_ps_fh_7, '>', $temp_file_ps_fh_7 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_7} $output_ps_fh_7;
close $fh_ps_fh_7 or croak "Close failed: $ERRNO\n";
my $temp_file_ps_fh_8 = q{/tmp} . '/process_sub_fh_8.tmp';
my $output_ps_fh_8;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'echo -e "value1\\nvalue2"');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_8 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_8 = dirname($temp_file_ps_fh_8);
if (!-d $temp_dir_fh_8) { make_path($temp_dir_fh_8); }
open my $fh_ps_fh_8, '>', $temp_file_ps_fh_8 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_8} $output_ps_fh_8;
close $fh_ps_fh_8 or croak "Close failed: $ERRNO\n";
my $paste_result_171 = do {
my @paste_file1_lines_fh_9;
my @paste_file2_lines_fh_9;
if (open my $fh1, '<', $temp_file_ps_fh_7) {
    while (my $line = <$fh1>) {
        chomp $line;
        push @paste_file1_lines_fh_9, $line;
    }
    close $fh1 or croak "Close failed: $OS_ERROR";
}
if (open my $fh2, '<', $temp_file_ps_fh_8) {
    while (my $line = <$fh2>) {
        chomp $line;
        push @paste_file2_lines_fh_9, $line;
    }
    close $fh2 or croak "Close failed: $OS_ERROR";
}
my $max_lines = scalar @paste_file1_lines_fh_9 > scalar @paste_file2_lines_fh_9 ? scalar @paste_file1_lines_fh_9 : scalar @paste_file2_lines_fh_9;
my $paste_output = q{};
for my $i (0..$max_lines-1) {
    my $line1 = $i < scalar @paste_file1_lines_fh_9 ? $paste_file1_lines_fh_9[$i] : q{};
    my $line2 = $i < scalar @paste_file2_lines_fh_9 ? $paste_file2_lines_fh_9[$i] : q{};
    $paste_output .= "$line1\t$line2\n";
}
$paste_output
};
print $paste_result_171;

exit $main_exit_code;

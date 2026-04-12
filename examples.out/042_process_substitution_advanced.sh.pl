#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== More process substitution examples ==\n";
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
    {
    my $output_11;
    my $output_printed_11;
    my $pipeline_success_11 = 1;
    $output_11 .= "a\nc\nb";
if ( !($output_11 =~ m{\n\z}msx) ) { $output_11 .= "\n"; }

        my @sort_lines_11_1 = split /\n/msx, $output_11;
    my @sort_sorted_11_1 = sort @sort_lines_11_1;
    my $output_11_1 = join "\n", @sort_sorted_11_1;
    if ($output_11_1 ne q{} && !($output_11_1 =~ m{\n\z}msx)) {
    $output_11_1 .= "\n";
    }
    $output_11 = $output_11_1;
    $output_11 = $output_11_1;
    if ($output_11 ne q{} && !defined $output_printed_11) {
        print $output_11;
        if (!($output_11 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_11 ) { $main_exit_code = 1; }
    }

}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print $fh_ps_fh_1 $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
my $output_ps_fh_2;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_2 or croak "Cannot redirect STDOUT";
    {
    my $output_12;
    my $output_printed_12;
    my $pipeline_success_12 = 1;
    $output_12 .= "a\nb\nd";
if ( !($output_12 =~ m{\n\z}msx) ) { $output_12 .= "\n"; }

        my @sort_lines_12_1 = split /\n/msx, $output_12;
    my @sort_sorted_12_1 = sort @sort_lines_12_1;
    my $output_12_1 = join "\n", @sort_sorted_12_1;
    if ($output_12_1 ne q{} && !($output_12_1 =~ m{\n\z}msx)) {
    $output_12_1 .= "\n";
    }
    $output_12 = $output_12_1;
    $output_12 = $output_12_1;
    if ($output_12 ne q{} && !defined $output_printed_12) {
        print $output_12;
        if (!($output_12 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_12 ) { $main_exit_code = 1; }
    }

}
use File::Path qw(make_path);
my $temp_dir_fh_2 = dirname($temp_file_ps_fh_2);
if (!-d $temp_dir_fh_2) { make_path($temp_dir_fh_2); }
open my $fh_ps_fh_2, '>', $temp_file_ps_fh_2 or croak "Cannot create temp file: $ERRNO\n";
print $fh_ps_fh_2 $output_ps_fh_2;
close $fh_ps_fh_2 or croak "Close failed: $ERRNO\n";
$ENV{DIFF_TEMP_FILE1} = q{/tmp} . '/process_sub_fh_1.tmp';
$ENV{DIFF_TEMP_FILE2} = q{/tmp} . '/process_sub_fh_2.tmp';
my $diff_exit_code = 0;
my $diff_output = q{};
{
    my $diff_cmd = 'diff';
    my @diff_args = ("$temp_file_ps_fh_1", "$temp_file_ps_fh_2");
    my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
    if ($diff_pid) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $diff_output = <$diff_fh>;
        my $close_result = close $diff_fh; # Capture but ignore close result for diff
        $diff_exit_code = $CHILD_ERROR >> 8;
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
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', "echo -e \"name1\\nname2\"");
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_3 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
}
use File::Path qw(make_path);
my $temp_dir_fh_3 = dirname($temp_file_ps_fh_3);
if (!-d $temp_dir_fh_3) { make_path($temp_dir_fh_3); }
open my $fh_ps_fh_3, '>', $temp_file_ps_fh_3 or croak "Cannot create temp file: $ERRNO\n";
print $fh_ps_fh_3 $output_ps_fh_3;
close $fh_ps_fh_3 or croak "Close failed: $ERRNO\n";
my $temp_file_ps_fh_4 = q{/tmp} . '/process_sub_fh_4.tmp';
my $output_ps_fh_4;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', "echo -e \"value1\\nvalue2\"");
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_4 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
}
use File::Path qw(make_path);
my $temp_dir_fh_4 = dirname($temp_file_ps_fh_4);
if (!-d $temp_dir_fh_4) { make_path($temp_dir_fh_4); }
open my $fh_ps_fh_4, '>', $temp_file_ps_fh_4 or croak "Cannot create temp file: $ERRNO\n";
print $fh_ps_fh_4 $output_ps_fh_4;
close $fh_ps_fh_4 or croak "Close failed: $ERRNO\n";
my @paste_file1_lines_fh_5;
my @paste_file2_lines_fh_5;
if (open my $fh1, '<', $temp_file_ps_fh_3) {
    while (my $line = <$fh1>) {
        chomp $line;
        push @paste_file1_lines_fh_5, $line;
    }
    close $fh1 or croak "Close failed: $OS_ERROR";
}
if (open my $fh2, '<', $temp_file_ps_fh_4) {
    while (my $line = <$fh2>) {
        chomp $line;
        push @paste_file2_lines_fh_5, $line;
    }
    close $fh2 or croak "Close failed: $OS_ERROR";
}
my $max_lines = scalar @paste_file1_lines_fh_5 > scalar @paste_file2_lines_fh_5 ? scalar @paste_file1_lines_fh_5 : scalar @paste_file2_lines_fh_5;
my $paste_output = q{};
for my $i (0..$max_lines-1) {
    my $line1 = $i < scalar @paste_file1_lines_fh_5 ? $paste_file1_lines_fh_5[$i] : q{};
    my $line2 = $i < scalar @paste_file2_lines_fh_5 ? $paste_file2_lines_fh_5[$i] : q{};
    $paste_output .= "$line1\t$line2\n";
}
print $paste_output;

exit $main_exit_code;

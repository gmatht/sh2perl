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
my $__set_e        = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '042_process_substitution_advanced.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== More process substitution examples ==\n";
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
    my $output_253 = q{};
    my $output_printed_253;
    {
        my $pipeline_success_253 = 1;
        $output_253 .= "a\nc\nb";
    if ( !($output_253 =~ m{\n\z}msx) ) { $output_253 .= "\n"; }
    $CHILD_ERROR = 0;
            my @sort_lines_253_1 = split /\n/msx, $output_253;
        my @sort_sorted_253_1 = sort @sort_lines_253_1;
        my $output_253_1 = join "\n", @sort_sorted_253_1;
        if ($output_253_1 ne q{} && !($output_253_1 =~ m{\n\z}msx)) {
        $output_253_1 .= "\n";
        }
        $output_253 = $output_253_1;
        $output_253 = $output_253_1;
        if ($output_253 ne q{} && !defined $output_printed_253) {
            print $output_253;
            if (!($output_253 =~ m{\n\z}msx)) {
                print "\n";
            }
        }
        if ( !$pipeline_success_253 ) { $main_exit_code = 1; }
        }
}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_1} $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_1 or croak "Cannot open process substitution: $ERRNO\n";
my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
my $output_ps_fh_2;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_2 or croak "Cannot redirect STDOUT";
    my $output_254 = q{};
    my $output_printed_254;
    {
        my $pipeline_success_254 = 1;
        $output_254 .= "a\nb\nd";
    if ( !($output_254 =~ m{\n\z}msx) ) { $output_254 .= "\n"; }
    $CHILD_ERROR = 0;
            my @sort_lines_254_1 = split /\n/msx, $output_254;
        my @sort_sorted_254_1 = sort @sort_lines_254_1;
        my $output_254_1 = join "\n", @sort_sorted_254_1;
        if ($output_254_1 ne q{} && !($output_254_1 =~ m{\n\z}msx)) {
        $output_254_1 .= "\n";
        }
        $output_254 = $output_254_1;
        $output_254 = $output_254_1;
        if ($output_254 ne q{} && !defined $output_printed_254) {
            print $output_254;
            if (!($output_254 =~ m{\n\z}msx)) {
                print "\n";
            }
        }
        if ( !$pipeline_success_254 ) { $main_exit_code = 1; }
        }
}
use File::Path qw(make_path);
my $temp_dir_fh_2 = dirname($temp_file_ps_fh_2);
if (!-d $temp_dir_fh_2) { make_path($temp_dir_fh_2); }
open my $fh_ps_fh_2, '>', $temp_file_ps_fh_2 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_2} $output_ps_fh_2;
close $fh_ps_fh_2 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_2 or croak "Cannot open process substitution: $ERRNO\n";
$ENV{DIFF_TEMP_FILE1} = q{/tmp} . '/process_sub_fh_1.tmp';
$ENV{DIFF_TEMP_FILE2} = q{/tmp} . '/process_sub_fh_2.tmp';
my $diff_output = q{};
{
    my $diff_cmd = 'diff';
    my @diff_args = ($temp_file_ps_fh_1, $temp_file_ps_fh_2);
    my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
    if ($diff_pid) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $diff_output = <$diff_fh>;
        close $diff_fh;
        $CHILD_ERROR = $? >> 8;
    } else {
        carp "Cannot execute diff command: $OS_ERROR";
        $diff_output = q{};
        $CHILD_ERROR = 1;
    }
}
print $diff_output;
if ($CHILD_ERROR != 0) {
        print "Files differ\n";
}
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_3 or croak "Cannot redirect STDOUT";
    my $output_255 = q{};
    my $output_printed_255;
    print "name1\nname2" . "\n";
    $CHILD_ERROR = 0;
if ($output_255 ne q{} && !$output_printed_255) {
    print $output_255;
}
}
use File::Path qw(make_path);
my $temp_dir_fh_3 = dirname($temp_file_ps_fh_3);
if (!-d $temp_dir_fh_3) { make_path($temp_dir_fh_3); }
open my $fh_ps_fh_3, '>', $temp_file_ps_fh_3 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_3} $output_ps_fh_3;
close $fh_ps_fh_3 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_3 or croak "Cannot open process substitution: $ERRNO\n";
my $temp_file_ps_fh_4 = q{/tmp} . '/process_sub_fh_4.tmp';
my $output_ps_fh_4;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_4 or croak "Cannot redirect STDOUT";
    my $output_256 = q{};
    my $output_printed_256;
    print "value1\nvalue2" . "\n";
    $CHILD_ERROR = 0;
if ($output_256 ne q{} && !$output_printed_256) {
    print $output_256;
}
}
use File::Path qw(make_path);
my $temp_dir_fh_4 = dirname($temp_file_ps_fh_4);
if (!-d $temp_dir_fh_4) { make_path($temp_dir_fh_4); }
open my $fh_ps_fh_4, '>', $temp_file_ps_fh_4 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_4} $output_ps_fh_4;
close $fh_ps_fh_4 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_4 or croak "Cannot open process substitution: $ERRNO\n";
my $paste_result_257 = do {
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
$paste_output
}
;
print $paste_result_257;

exit $main_exit_code;

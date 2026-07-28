#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '042_process_substitution_advanced.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== More process substitution examples ==";
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
    my $output_242 = q{};
    my $output_printed_242;
    do {
        my $pipeline_success_242 = 1;
        $output_242 .= "a\nc\nb";
    if ( !($output_242 =~ m{\n\z}) ) { $output_242 .= "\n"; }
            my @sort_lines_242_1 = split /\n/, $output_242;
        my @sort_sorted_242_1 = sort @sort_lines_242_1;
        my $output_242_1 = join("\n", @sort_sorted_242_1);
        $output_242 = $output_242_1;
        $output_242 = $output_242_1;
        if ($output_242 ne q{} && !defined $output_printed_242) {
            print $output_242;
            if (!($output_242 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_242 ) { $main_exit_code = 1; }
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
    my $output_243 = q{};
    my $output_printed_243;
    do {
        my $pipeline_success_243 = 1;
        $output_243 .= "a\nb\nd";
    if ( !($output_243 =~ m{\n\z}) ) { $output_243 .= "\n"; }
            my @sort_lines_243_1 = split /\n/, $output_243;
        my @sort_sorted_243_1 = sort @sort_lines_243_1;
        my $output_243_1 = join("\n", @sort_sorted_243_1);
        $output_243 = $output_243_1;
        $output_243 = $output_243_1;
        if ($output_243 ne q{} && !defined $output_printed_243) {
            print $output_243;
            if (!($output_243 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_243 ) { $main_exit_code = 1; }
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
my $diff_output = qx{'diff' $temp_file_ps_fh_1 $temp_file_ps_fh_2};
chomp $diff_output;
say $diff_output;
if ($CHILD_ERROR != 0) {
        say "Files differ";
}
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_3 or croak "Cannot redirect STDOUT";
    my $output_244 = q{};
    my $output_printed_244;
    print "name1\nname2" . "\n";
if ($output_244 ne q{} && !$output_printed_244) {
    print $output_244;
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
    my $output_245 = q{};
    my $output_printed_245;
    print "value1\nvalue2" . "\n";
if ($output_245 ne q{} && !$output_printed_245) {
    print $output_245;
}
}
use File::Path qw(make_path);
my $temp_dir_fh_4 = dirname($temp_file_ps_fh_4);
if (!-d $temp_dir_fh_4) { make_path($temp_dir_fh_4); }
open my $fh_ps_fh_4, '>', $temp_file_ps_fh_4 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_4} $output_ps_fh_4;
close $fh_ps_fh_4 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_4 or croak "Cannot open process substitution: $ERRNO\n";
my $paste_result_246 = do {
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
print $paste_result_246;

exit $main_exit_code;

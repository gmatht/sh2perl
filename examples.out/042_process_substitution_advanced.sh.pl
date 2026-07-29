#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $main_exit_code = 0;
my $__set_e = 0;
my $output = '';
our $CHILD_ERROR;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== More process substitution examples ==\n";
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
    my $output_0 = q{};
    my $output_printed_0;
    my $output_1 = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "a\\\\nc\\\\nb" | sort}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
    print $output_1, "\n";
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
    my $output_2 = q{};
    my $output_printed_2;
    my $output_3 = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "a\\\\nb\\\\nd" | sort}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
    print $output_3, "\n";
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
my $diff_output = do { my $__out = q{}; if (@ARGV) { local $/; for my $__f (@ARGV) { open(my $__fh, q{<}, $__f) and do { $__out .= <$__fh>; close $__fh } } } $__out; };
print $diff_output, "\n";
if ($CHILD_ERROR != 0) {
        print "Files differ\n";
}
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_3 or croak "Cannot redirect STDOUT";
    my $output_4 = q{};
    my $output_printed_4;
    print "name1\nname2" . "\n";
if ($output_4 ne q{} && !$output_printed_4) {
    print $output_4;
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
    my $output_5 = q{};
    my $output_printed_5;
    print "value1\nvalue2" . "\n";
if ($output_5 ne q{} && !$output_printed_5) {
    print $output_5;
}
}
use File::Path qw(make_path);
my $temp_dir_fh_4 = dirname($temp_file_ps_fh_4);
if (!-d $temp_dir_fh_4) { make_path($temp_dir_fh_4); }
open my $fh_ps_fh_4, '>', $temp_file_ps_fh_4 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_4} $output_ps_fh_4;
close $fh_ps_fh_4 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_4 or croak "Cannot open process substitution: $ERRNO\n";
my $paste_result_6 = do {
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
print $paste_result_6;
my $result = "ok";
print "status: " . ${result}, "\n";

exit $main_exit_code;


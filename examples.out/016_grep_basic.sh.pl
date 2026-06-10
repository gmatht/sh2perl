#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = '016_grep_basic.sh';
my $grep_result_166;
my @grep_lines_166 = ();
my @grep_filenames_166 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_166, $line;
        push @grep_filenames_166, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_166 = grep { /pattern/msx } @grep_lines_166;
$grep_result_166 = join "\n", @grep_filtered_166;
if (!($grep_result_166 =~ m{\n\z}msx || $grep_result_166 eq q{})) {
    $grep_result_166 .= "\n";
}
print $grep_result_166;
$CHILD_ERROR = scalar @grep_filtered_166 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
{
    my $output_167 = q{};
    my $output_printed_167;
    my $pipeline_success_167 = 1;
    $output_167 .= 'HELLO world' . "\n";
if ( !($output_167 =~ m{\n\z}msx) ) { $output_167 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_167_1;
    my @grep_lines_167_1 = split /\n/msx, $output_167;
    my @grep_filtered_167_1 = grep { /hello/msxi } @grep_lines_167_1;
    $grep_result_167_1 = join "\n", @grep_filtered_167_1;
    if (!($grep_result_167_1 =~ m{\n\z}msx || $grep_result_167_1 eq q{})) {
    $grep_result_167_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_167_1 > 0 ? 0 : 1;
    $output_167 = $grep_result_167_1;
    $output_167 = $grep_result_167_1;
    if ((scalar @grep_filtered_167_1) == 0) {
        $pipeline_success_167 = 0;
    }
    if ($output_167 ne q{} && !defined $output_printed_167) {
        print $output_167;
        if (!($output_167 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_167 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_168 = q{};
    my $output_printed_168;
    my $pipeline_success_168 = 1;
    $output_168 .= "line1\nline2\nline3";
if ( !($output_168 =~ m{\n\z}msx) ) { $output_168 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_168_1;
    my @grep_lines_168_1 = split /\n/msx, $output_168;
    my @grep_filtered_168_1 = grep { !/line2/msx } @grep_lines_168_1;
    $grep_result_168_1 = join "\n", @grep_filtered_168_1;
    if (!($grep_result_168_1 =~ m{\n\z}msx || $grep_result_168_1 eq q{})) {
    $grep_result_168_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_168_1 > 0 ? 0 : 1;
    $output_168 = $grep_result_168_1;
    $output_168 = $grep_result_168_1;
    if ((scalar @grep_filtered_168_1) == 0) {
        $pipeline_success_168 = 0;
    }
    if ($output_168 ne q{} && !defined $output_printed_168) {
        print $output_168;
        if (!($output_168 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_168 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
{
    my $output_169 = q{};
    my $output_printed_169;
    my $pipeline_success_169 = 1;
    $output_169 .= "first\nsecond\nthird";
if ( !($output_169 =~ m{\n\z}msx) ) { $output_169 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_169_1;
    my @grep_lines_169_1 = split /\n/msx, $output_169;
    my @grep_filtered_169_1 = grep { /second/msx } @grep_lines_169_1;
    my @grep_numbered_169_1;
    for my $i (0..@grep_lines_169_1-1) {
    if (scalar grep { $_ eq $grep_lines_169_1[$i] } @grep_filtered_169_1) {
    push @grep_numbered_169_1, sprintf "%d:%s", $i + 1, $grep_lines_169_1[$i];
    }
    }
    $grep_result_169_1 = join "\n", @grep_numbered_169_1;
    $CHILD_ERROR = scalar @grep_filtered_169_1 > 0 ? 0 : 1;
    $output_169 = $grep_result_169_1;
    $output_169 = $grep_result_169_1;
    if ((scalar @grep_filtered_169_1) == 0) {
        $pipeline_success_169 = 0;
    }
    if ($output_169 ne q{} && !defined $output_printed_169) {
        print $output_169;
        if (!($output_169 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_169 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_170 = q{};
    my $output_printed_170;
    my $pipeline_success_170 = 1;
    $output_170 .= "match\nno match\nmatch again";
if ( !($output_170 =~ m{\n\z}msx) ) { $output_170 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_170_1;
    my @grep_lines_170_1 = split /\n/msx, $output_170;
    my @grep_filtered_170_1 = grep { /match/msx } @grep_lines_170_1;
    $grep_result_170_1 = scalar @grep_filtered_170_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_170_1 > 0 ? 0 : 1;
    $output_170 = $grep_result_170_1;
    $output_170 = $grep_result_170_1;
    if ((scalar @grep_filtered_170_1) == 0) {
        $pipeline_success_170 = 0;
    }
    if ($output_170 ne q{} && !defined $output_printed_170) {
        print $output_170;
        if (!($output_170 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_170 ) { $main_exit_code = 1; }
    }
{
    my $output_171 = q{};
    my $output_printed_171;
    my $pipeline_success_171 = 1;
    $output_171 .= 'text with pattern123 in it' . "\n";
if ( !($output_171 =~ m{\n\z}msx) ) { $output_171 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_171_1;
    my @grep_lines_171_1 = split /\n/msx, $output_171;
    my @grep_filtered_171_1 = grep { /pattern[0-9]+/msx } @grep_lines_171_1;
    my @grep_matches_171_1;
    foreach my $line (@grep_filtered_171_1) {
    if ($line =~ /(pattern[0-9]+)/msx) {
    push @grep_matches_171_1, $1;
    }
    }
    $grep_result_171_1 = join "\n", @grep_matches_171_1;
    $CHILD_ERROR = scalar @grep_filtered_171_1 > 0 ? 0 : 1;
    $output_171 = $grep_result_171_1;
    $output_171 = $grep_result_171_1;
    if ((scalar @grep_filtered_171_1) == 0) {
        $pipeline_success_171 = 0;
    }
    if ($output_171 ne q{} && !defined $output_printed_171) {
        print $output_171;
        if (!($output_171 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_171 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

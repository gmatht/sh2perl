#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_0 = grep { /pattern/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
print $grep_result_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
{
    my $output_1 = q{};
    my $output_printed_1;
    my $pipeline_success_1 = 1;
    $output_1 .= 'HELLO world' . "\n";
if ( !($output_1 =~ m{\n\z}msx) ) { $output_1 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_1_1;
    my @grep_lines_1_1 = split /\n/msx, $output_1;
    my @grep_filtered_1_1 = grep { /hello/msxi } @grep_lines_1_1;
    $grep_result_1_1 = join "\n", @grep_filtered_1_1;
    if (!($grep_result_1_1 =~ m{\n\z}msx || $grep_result_1_1 eq q{})) {
    $grep_result_1_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_1_1 > 0 ? 0 : 1;
    $output_1 = $grep_result_1_1;
    $output_1 = $grep_result_1_1;
    if ((scalar @grep_filtered_1_1) == 0) {
        $pipeline_success_1 = 0;
    }
    if ($output_1 ne q{} && !defined $output_printed_1) {
        print $output_1;
        if (!($output_1 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_2 = q{};
    my $output_printed_2;
    my $pipeline_success_2 = 1;
    $output_2 .= "line1\nline2\nline3";
if ( !($output_2 =~ m{\n\z}msx) ) { $output_2 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_2_1;
    my @grep_lines_2_1 = split /\n/msx, $output_2;
    my @grep_filtered_2_1 = grep { !/line2/msx } @grep_lines_2_1;
    $grep_result_2_1 = join "\n", @grep_filtered_2_1;
    if (!($grep_result_2_1 =~ m{\n\z}msx || $grep_result_2_1 eq q{})) {
    $grep_result_2_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_2_1 > 0 ? 0 : 1;
    $output_2 = $grep_result_2_1;
    $output_2 = $grep_result_2_1;
    if ((scalar @grep_filtered_2_1) == 0) {
        $pipeline_success_2 = 0;
    }
    if ($output_2 ne q{} && !defined $output_printed_2) {
        print $output_2;
        if (!($output_2 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_2 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
{
    my $output_3 = q{};
    my $output_printed_3;
    my $pipeline_success_3 = 1;
    $output_3 .= "first\nsecond\nthird";
if ( !($output_3 =~ m{\n\z}msx) ) { $output_3 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_3_1;
    my @grep_lines_3_1 = split /\n/msx, $output_3;
    my @grep_filtered_3_1 = grep { /second/msx } @grep_lines_3_1;
    my @grep_numbered_3_1;
    for my $i (0..@grep_lines_3_1-1) {
    if (scalar grep { $_ eq $grep_lines_3_1[$i] } @grep_filtered_3_1) {
    push @grep_numbered_3_1, sprintf "%d:%s", $i + 1, $grep_lines_3_1[$i];
    }
    }
    $grep_result_3_1 = join "\n", @grep_numbered_3_1;
    $CHILD_ERROR = scalar @grep_filtered_3_1 > 0 ? 0 : 1;
    $output_3 = $grep_result_3_1;
    $output_3 = $grep_result_3_1;
    if ((scalar @grep_filtered_3_1) == 0) {
        $pipeline_success_3 = 0;
    }
    if ($output_3 ne q{} && !defined $output_printed_3) {
        print $output_3;
        if (!($output_3 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_3 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_4 = q{};
    my $output_printed_4;
    my $pipeline_success_4 = 1;
    $output_4 .= "match\nno match\nmatch again";
if ( !($output_4 =~ m{\n\z}msx) ) { $output_4 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_4_1;
    my @grep_lines_4_1 = split /\n/msx, $output_4;
    my @grep_filtered_4_1 = grep { /match/msx } @grep_lines_4_1;
    $grep_result_4_1 = scalar @grep_filtered_4_1;
    $CHILD_ERROR = scalar @grep_filtered_4_1 > 0 ? 0 : 1;
    $output_4 = $grep_result_4_1;
    $output_4 = $grep_result_4_1;
    if ((scalar @grep_filtered_4_1) == 0) {
        $pipeline_success_4 = 0;
    }
    if ($output_4 ne q{} && !defined $output_printed_4) {
        print $output_4;
        if (!($output_4 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_4 ) { $main_exit_code = 1; }
    }
{
    my $output_5 = q{};
    my $output_printed_5;
    my $pipeline_success_5 = 1;
    $output_5 .= 'text with pattern123 in it' . "\n";
if ( !($output_5 =~ m{\n\z}msx) ) { $output_5 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_5_1;
    my @grep_lines_5_1 = split /\n/msx, $output_5;
    my @grep_filtered_5_1 = grep { /pattern[0-9]+/msx } @grep_lines_5_1;
    my @grep_matches_5_1;
    foreach my $line (@grep_filtered_5_1) {
    if ($line =~ /(pattern[0-9]+)/msx) {
    push @grep_matches_5_1, $1;
    }
    }
    $grep_result_5_1 = join "\n", @grep_matches_5_1;
    $CHILD_ERROR = scalar @grep_filtered_5_1 > 0 ? 0 : 1;
    $output_5 = $grep_result_5_1;
    $output_5 = $grep_result_5_1;
    if ((scalar @grep_filtered_5_1) == 0) {
        $pipeline_success_5 = 0;
    }
    if ($output_5 ne q{} && !defined $output_printed_5) {
        print $output_5;
        if (!($output_5 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_5 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

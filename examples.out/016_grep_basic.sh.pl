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
my $grep_result_177;
my @grep_lines_177 = ();
my @grep_filenames_177 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_177, $line;
        push @grep_filenames_177, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_177 = grep { /pattern/msx } @grep_lines_177;
$grep_result_177 = join "\n", @grep_filtered_177;
if (!($grep_result_177 =~ m{\n\z}msx || $grep_result_177 eq q{})) {
    $grep_result_177 .= "\n";
}
print $grep_result_177;
$CHILD_ERROR = scalar @grep_filtered_177 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
{
    my $output_178 = q{};
    my $output_printed_178;
    my $pipeline_success_178 = 1;
    $output_178 .= 'HELLO world' . "\n";
if ( !($output_178 =~ m{\n\z}msx) ) { $output_178 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_178_1;
    my @grep_lines_178_1 = split /\n/msx, $output_178;
    my @grep_filtered_178_1 = grep { /hello/msxi } @grep_lines_178_1;
    $grep_result_178_1 = join "\n", @grep_filtered_178_1;
    if (!($grep_result_178_1 =~ m{\n\z}msx || $grep_result_178_1 eq q{})) {
    $grep_result_178_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_178_1 > 0 ? 0 : 1;
    $output_178 = $grep_result_178_1;
    $output_178 = $grep_result_178_1;
    if ((scalar @grep_filtered_178_1) == 0) {
        $pipeline_success_178 = 0;
    }
    if ($output_178 ne q{} && !defined $output_printed_178) {
        print $output_178;
        if (!($output_178 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_178 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_179 = q{};
    my $output_printed_179;
    my $pipeline_success_179 = 1;
    $output_179 .= "line1\nline2\nline3";
if ( !($output_179 =~ m{\n\z}msx) ) { $output_179 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_179_1;
    my @grep_lines_179_1 = split /\n/msx, $output_179;
    my @grep_filtered_179_1 = grep { !/line2/msx } @grep_lines_179_1;
    $grep_result_179_1 = join "\n", @grep_filtered_179_1;
    if (!($grep_result_179_1 =~ m{\n\z}msx || $grep_result_179_1 eq q{})) {
    $grep_result_179_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_179_1 > 0 ? 0 : 1;
    $output_179 = $grep_result_179_1;
    $output_179 = $grep_result_179_1;
    if ((scalar @grep_filtered_179_1) == 0) {
        $pipeline_success_179 = 0;
    }
    if ($output_179 ne q{} && !defined $output_printed_179) {
        print $output_179;
        if (!($output_179 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_179 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
{
    my $output_180 = q{};
    my $output_printed_180;
    my $pipeline_success_180 = 1;
    $output_180 .= "first\nsecond\nthird";
if ( !($output_180 =~ m{\n\z}msx) ) { $output_180 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_180_1;
    my @grep_lines_180_1 = split /\n/msx, $output_180;
    my @grep_filtered_180_1 = grep { /second/msx } @grep_lines_180_1;
    my @grep_numbered_180_1;
    for my $i (0..@grep_lines_180_1-1) {
    if (scalar grep { $_ eq $grep_lines_180_1[$i] } @grep_filtered_180_1) {
    push @grep_numbered_180_1, sprintf "%d:%s", $i + 1, $grep_lines_180_1[$i];
    }
    }
    $grep_result_180_1 = join "\n", @grep_numbered_180_1;
    $CHILD_ERROR = scalar @grep_filtered_180_1 > 0 ? 0 : 1;
    $output_180 = $grep_result_180_1;
    $output_180 = $grep_result_180_1;
    if ((scalar @grep_filtered_180_1) == 0) {
        $pipeline_success_180 = 0;
    }
    if ($output_180 ne q{} && !defined $output_printed_180) {
        print $output_180;
        if (!($output_180 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_180 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_181 = q{};
    my $output_printed_181;
    my $pipeline_success_181 = 1;
    $output_181 .= "match\nno match\nmatch again";
if ( !($output_181 =~ m{\n\z}msx) ) { $output_181 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_181_1;
    my @grep_lines_181_1 = split /\n/msx, $output_181;
    my @grep_filtered_181_1 = grep { /match/msx } @grep_lines_181_1;
    $grep_result_181_1 = scalar @grep_filtered_181_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_181_1 > 0 ? 0 : 1;
    $output_181 = $grep_result_181_1;
    $output_181 = $grep_result_181_1;
    if ((scalar @grep_filtered_181_1) == 0) {
        $pipeline_success_181 = 0;
    }
    if ($output_181 ne q{} && !defined $output_printed_181) {
        print $output_181;
        if (!($output_181 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_181 ) { $main_exit_code = 1; }
    }
{
    my $output_182 = q{};
    my $output_printed_182;
    my $pipeline_success_182 = 1;
    $output_182 .= 'text with pattern123 in it' . "\n";
if ( !($output_182 =~ m{\n\z}msx) ) { $output_182 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_182_1;
    my @grep_lines_182_1 = split /\n/msx, $output_182;
    my @grep_filtered_182_1 = grep { /pattern[0-9]+/msx } @grep_lines_182_1;
    my @grep_matches_182_1;
    foreach my $line (@grep_filtered_182_1) {
    if ($line =~ /(pattern[0-9]+)/msx) {
    push @grep_matches_182_1, $1;
    }
    }
    $grep_result_182_1 = join "\n", @grep_matches_182_1;
    $CHILD_ERROR = scalar @grep_filtered_182_1 > 0 ? 0 : 1;
    $output_182 = $grep_result_182_1;
    $output_182 = $grep_result_182_1;
    if ((scalar @grep_filtered_182_1) == 0) {
        $pipeline_success_182 = 0;
    }
    if ($output_182 ne q{} && !defined $output_printed_182) {
        print $output_182;
        if (!($output_182 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_182 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

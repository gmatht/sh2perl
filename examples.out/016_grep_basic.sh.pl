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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '016_grep_basic.sh';
my $grep_result_188;
my @grep_lines_188 = ();
my @grep_filenames_188 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_188, $line;
        push @grep_filenames_188, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_188 = grep { /pattern/msx } @grep_lines_188;
$grep_result_188 = join "\n", @grep_filtered_188;
if (!($grep_result_188 =~ m{\n\z}msx || $grep_result_188 eq q{})) {
    $grep_result_188 .= "\n";
}
print $grep_result_188;
$CHILD_ERROR = scalar @grep_filtered_188 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
{
    my $output_189 = q{};
    my $output_printed_189;
    my $pipeline_success_189 = 1;
    $output_189 .= 'HELLO world' . "\n";
if ( !($output_189 =~ m{\n\z}msx) ) { $output_189 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_189_1;
    my @grep_lines_189_1 = split /\n/msx, $output_189;
    my @grep_filtered_189_1 = grep { /hello/msxi } @grep_lines_189_1;
    $grep_result_189_1 = join "\n", @grep_filtered_189_1;
    if (!($grep_result_189_1 =~ m{\n\z}msx || $grep_result_189_1 eq q{})) {
    $grep_result_189_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_189_1 > 0 ? 0 : 1;
    $output_189 = $grep_result_189_1;
    $output_189 = $grep_result_189_1;
    if ((scalar @grep_filtered_189_1) == 0) {
        $pipeline_success_189 = 0;
    }
    if ($output_189 ne q{} && !defined $output_printed_189) {
        print $output_189;
        if (!($output_189 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_189 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_190 = q{};
    my $output_printed_190;
    my $pipeline_success_190 = 1;
    $output_190 .= "line1\nline2\nline3";
if ( !($output_190 =~ m{\n\z}msx) ) { $output_190 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_190_1;
    my @grep_lines_190_1 = split /\n/msx, $output_190;
    my @grep_filtered_190_1 = grep { !/line2/msx } @grep_lines_190_1;
    $grep_result_190_1 = join "\n", @grep_filtered_190_1;
    if (!($grep_result_190_1 =~ m{\n\z}msx || $grep_result_190_1 eq q{})) {
    $grep_result_190_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_190_1 > 0 ? 0 : 1;
    $output_190 = $grep_result_190_1;
    $output_190 = $grep_result_190_1;
    if ((scalar @grep_filtered_190_1) == 0) {
        $pipeline_success_190 = 0;
    }
    if ($output_190 ne q{} && !defined $output_printed_190) {
        print $output_190;
        if (!($output_190 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_190 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
{
    my $output_191 = q{};
    my $output_printed_191;
    my $pipeline_success_191 = 1;
    $output_191 .= "first\nsecond\nthird";
if ( !($output_191 =~ m{\n\z}msx) ) { $output_191 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_191_1;
    my @grep_lines_191_1 = split /\n/msx, $output_191;
    my @grep_filtered_191_1 = grep { /second/msx } @grep_lines_191_1;
    my @grep_numbered_191_1;
    for my $i (0..@grep_lines_191_1-1) {
    if (scalar grep { $_ eq $grep_lines_191_1[$i] } @grep_filtered_191_1) {
    push @grep_numbered_191_1, sprintf "%d:%s", $i + 1, $grep_lines_191_1[$i];
    }
    }
    $grep_result_191_1 = join "\n", @grep_numbered_191_1;
    $CHILD_ERROR = scalar @grep_filtered_191_1 > 0 ? 0 : 1;
    $output_191 = $grep_result_191_1;
    $output_191 = $grep_result_191_1;
    if ((scalar @grep_filtered_191_1) == 0) {
        $pipeline_success_191 = 0;
    }
    if ($output_191 ne q{} && !defined $output_printed_191) {
        print $output_191;
        if (!($output_191 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_191 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_192 = q{};
    my $output_printed_192;
    my $pipeline_success_192 = 1;
    $output_192 .= "match\nno match\nmatch again";
if ( !($output_192 =~ m{\n\z}msx) ) { $output_192 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_192_1;
    my @grep_lines_192_1 = split /\n/msx, $output_192;
    my @grep_filtered_192_1 = grep { /match/msx } @grep_lines_192_1;
    $grep_result_192_1 = scalar @grep_filtered_192_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_192_1 > 0 ? 0 : 1;
    $output_192 = $grep_result_192_1;
    $output_192 = $grep_result_192_1;
    if ((scalar @grep_filtered_192_1) == 0) {
        $pipeline_success_192 = 0;
    }
    if ($output_192 ne q{} && !defined $output_printed_192) {
        print $output_192;
        if (!($output_192 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_192 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern123 in it" | grep -o "pattern[0-9]\+"
{
    my $output_193 = q{};
    my $output_printed_193;
    my $pipeline_success_193 = 1;
    $output_193 .= 'text with pattern123 in it' . "\n";
if ( !($output_193 =~ m{\n\z}msx) ) { $output_193 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_193_1;
    my @grep_lines_193_1 = split /\n/msx, $output_193;
    my @grep_filtered_193_1 = grep { /pattern[0-9]+/msx } @grep_lines_193_1;
    my @grep_matches_193_1;
    foreach my $line (@grep_filtered_193_1) {
    if ($line =~ /(pattern[0-9]+)/msx) {
    push @grep_matches_193_1, $1;
    }
    }
    $grep_result_193_1 = join "\n", @grep_matches_193_1;
    $CHILD_ERROR = scalar @grep_filtered_193_1 > 0 ? 0 : 1;
    $output_193 = $grep_result_193_1;
    $output_193 = $grep_result_193_1;
    if ((scalar @grep_filtered_193_1) == 0) {
        $pipeline_success_193 = 0;
    }
    if ($output_193 ne q{} && !defined $output_printed_193) {
        print $output_193;
        if (!($output_193 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_193 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

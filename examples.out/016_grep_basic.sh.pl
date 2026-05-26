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

my $grep_result_183;
my @grep_lines_183 = ();
my @grep_filenames_183 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_183, $line;
        push @grep_filenames_183, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_183 = grep { /pattern/msx } @grep_lines_183;
$grep_result_183 = join "\n", @grep_filtered_183;
if (!($grep_result_183 =~ m{\n\z}msx || $grep_result_183 eq q{})) {
    $grep_result_183 .= "\n";
}
print $grep_result_183;
$CHILD_ERROR = scalar @grep_filtered_183 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
{
    my $output_184 = q{};
    my $output_printed_184;
    my $pipeline_success_184 = 1;
    $output_184 .= 'HELLO world' . "\n";
if ( !($output_184 =~ m{\n\z}msx) ) { $output_184 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_184_1;
    my @grep_lines_184_1 = split /\n/msx, $output_184;
    my @grep_filtered_184_1 = grep { /hello/msxi } @grep_lines_184_1;
    $grep_result_184_1 = join "\n", @grep_filtered_184_1;
    if (!($grep_result_184_1 =~ m{\n\z}msx || $grep_result_184_1 eq q{})) {
    $grep_result_184_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_184_1 > 0 ? 0 : 1;
    $output_184 = $grep_result_184_1;
    $output_184 = $grep_result_184_1;
    if ((scalar @grep_filtered_184_1) == 0) {
        $pipeline_success_184 = 0;
    }
    if ($output_184 ne q{} && !defined $output_printed_184) {
        print $output_184;
        if (!($output_184 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_184 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_185 = q{};
    my $output_printed_185;
    my $pipeline_success_185 = 1;
    $output_185 .= "line1\nline2\nline3";
if ( !($output_185 =~ m{\n\z}msx) ) { $output_185 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_185_1;
    my @grep_lines_185_1 = split /\n/msx, $output_185;
    my @grep_filtered_185_1 = grep { !/line2/msx } @grep_lines_185_1;
    $grep_result_185_1 = join "\n", @grep_filtered_185_1;
    if (!($grep_result_185_1 =~ m{\n\z}msx || $grep_result_185_1 eq q{})) {
    $grep_result_185_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_185_1 > 0 ? 0 : 1;
    $output_185 = $grep_result_185_1;
    $output_185 = $grep_result_185_1;
    if ((scalar @grep_filtered_185_1) == 0) {
        $pipeline_success_185 = 0;
    }
    if ($output_185 ne q{} && !defined $output_printed_185) {
        print $output_185;
        if (!($output_185 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_185 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
{
    my $output_186 = q{};
    my $output_printed_186;
    my $pipeline_success_186 = 1;
    $output_186 .= "first\nsecond\nthird";
if ( !($output_186 =~ m{\n\z}msx) ) { $output_186 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_186_1;
    my @grep_lines_186_1 = split /\n/msx, $output_186;
    my @grep_filtered_186_1 = grep { /second/msx } @grep_lines_186_1;
    my @grep_numbered_186_1;
    for my $i (0..@grep_lines_186_1-1) {
    if (scalar grep { $_ eq $grep_lines_186_1[$i] } @grep_filtered_186_1) {
    push @grep_numbered_186_1, sprintf "%d:%s", $i + 1, $grep_lines_186_1[$i];
    }
    }
    $grep_result_186_1 = join "\n", @grep_numbered_186_1;
    $CHILD_ERROR = scalar @grep_filtered_186_1 > 0 ? 0 : 1;
    $output_186 = $grep_result_186_1;
    $output_186 = $grep_result_186_1;
    if ((scalar @grep_filtered_186_1) == 0) {
        $pipeline_success_186 = 0;
    }
    if ($output_186 ne q{} && !defined $output_printed_186) {
        print $output_186;
        if (!($output_186 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_186 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_187 = q{};
    my $output_printed_187;
    my $pipeline_success_187 = 1;
    $output_187 .= "match\nno match\nmatch again";
if ( !($output_187 =~ m{\n\z}msx) ) { $output_187 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_187_1;
    my @grep_lines_187_1 = split /\n/msx, $output_187;
    my @grep_filtered_187_1 = grep { /match/msx } @grep_lines_187_1;
    $grep_result_187_1 = scalar @grep_filtered_187_1;
    $CHILD_ERROR = scalar @grep_filtered_187_1 > 0 ? 0 : 1;
    $output_187 = $grep_result_187_1;
    $output_187 = $grep_result_187_1;
    if ((scalar @grep_filtered_187_1) == 0) {
        $pipeline_success_187 = 0;
    }
    if ($output_187 ne q{} && !defined $output_printed_187) {
        print $output_187;
        if (!($output_187 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_187 ) { $main_exit_code = 1; }
    }
{
    my $output_188 = q{};
    my $output_printed_188;
    my $pipeline_success_188 = 1;
    $output_188 .= 'text with pattern123 in it' . "\n";
if ( !($output_188 =~ m{\n\z}msx) ) { $output_188 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_188_1;
    my @grep_lines_188_1 = split /\n/msx, $output_188;
    my @grep_filtered_188_1 = grep { /pattern[0-9]+/msx } @grep_lines_188_1;
    my @grep_matches_188_1;
    foreach my $line (@grep_filtered_188_1) {
    if ($line =~ /(pattern[0-9]+)/msx) {
    push @grep_matches_188_1, $1;
    }
    }
    $grep_result_188_1 = join "\n", @grep_matches_188_1;
    $CHILD_ERROR = scalar @grep_filtered_188_1 > 0 ? 0 : 1;
    $output_188 = $grep_result_188_1;
    $output_188 = $grep_result_188_1;
    if ((scalar @grep_filtered_188_1) == 0) {
        $pipeline_success_188 = 0;
    }
    if ($output_188 ne q{} && !defined $output_printed_188) {
        print $output_188;
        if (!($output_188 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_188 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

my $grep_result_190;
my @grep_lines_190 = ();
my @grep_filenames_190 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_190, $line;
        push @grep_filenames_190, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_190 = grep { /pattern/msx } @grep_lines_190;
$grep_result_190 = join "\n", @grep_filtered_190;
if (!($grep_result_190 =~ m{\n\z}msx || $grep_result_190 eq q{})) {
    $grep_result_190 .= "\n";
}
print $grep_result_190;
$CHILD_ERROR = scalar @grep_filtered_190 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
{
    my $output_191;
    my $output_printed_191;
    my $pipeline_success_191 = 1;
    $output_191 .= "HELLO world\n";
if ( !($output_191 =~ m{\n\z}msx) ) { $output_191 .= "\n"; }

        my $grep_result_191_1;
    my @grep_lines_191_1 = split /\n/msx, $output_191;
    my @grep_filtered_191_1 = grep { /hello/msxi } @grep_lines_191_1;
    $grep_result_191_1 = join "\n", @grep_filtered_191_1;
    if (!($grep_result_191_1 =~ m{\n\z}msx || $grep_result_191_1 eq q{})) {
    $grep_result_191_1 .= "\n";
    }
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
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_192;
    my $output_printed_192;
    my $pipeline_success_192 = 1;
    $output_192 .= "line1\nline2\nline3";
if ( !($output_192 =~ m{\n\z}msx) ) { $output_192 .= "\n"; }

        my $grep_result_192_1;
    my @grep_lines_192_1 = split /\n/msx, $output_192;
    my @grep_filtered_192_1 = grep { !/line2/msx } @grep_lines_192_1;
    $grep_result_192_1 = join "\n", @grep_filtered_192_1;
    if (!($grep_result_192_1 =~ m{\n\z}msx || $grep_result_192_1 eq q{})) {
    $grep_result_192_1 .= "\n";
    }
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
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
{
    my $output_193;
    my $output_printed_193;
    my $pipeline_success_193 = 1;
    $output_193 .= "first\nsecond\nthird";
if ( !($output_193 =~ m{\n\z}msx) ) { $output_193 .= "\n"; }

        my $grep_result_193_1;
    my @grep_lines_193_1 = split /\n/msx, $output_193;
    my @grep_filtered_193_1 = grep { /second/msx } @grep_lines_193_1;
    my @grep_numbered_193_1;
    for my $i (0..@grep_lines_193_1-1) {
    if (scalar grep { $_ eq $grep_lines_193_1[$i] } @grep_filtered_193_1) {
    push @grep_numbered_193_1, sprintf "%d:%s", $i + 1, $grep_lines_193_1[$i];
    }
    }
    $grep_result_193_1 = join "\n", @grep_numbered_193_1;
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
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_194;
    my $output_printed_194;
    my $pipeline_success_194 = 1;
    $output_194 .= "match\nno match\nmatch again";
if ( !($output_194 =~ m{\n\z}msx) ) { $output_194 .= "\n"; }

        my $grep_result_194_1;
    my @grep_lines_194_1 = split /\n/msx, $output_194;
    my @grep_filtered_194_1 = grep { /match/msx } @grep_lines_194_1;
    $grep_result_194_1 = scalar @grep_filtered_194_1;
    $CHILD_ERROR = scalar @grep_filtered_194_1 > 0 ? 0 : 1;
    $output_194 = $grep_result_194_1;
    $output_194 = $grep_result_194_1;
    if ((scalar @grep_filtered_194_1) == 0) {
        $pipeline_success_194 = 0;
    }
    if ($output_194 ne q{} && !defined $output_printed_194) {
        print $output_194;
        if (!($output_194 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_194 ) { $main_exit_code = 1; }
    }
{
    my $output_195;
    my $output_printed_195;
    my $pipeline_success_195 = 1;
    $output_195 .= "text with pattern123 in it\n";
if ( !($output_195 =~ m{\n\z}msx) ) { $output_195 .= "\n"; }

        my $grep_result_195_1;
    my @grep_lines_195_1 = split /\n/msx, $output_195;
    my @grep_filtered_195_1 = grep { /pattern[0-9]+/msx } @grep_lines_195_1;
    my @grep_matches_195_1;
    foreach my $line (@grep_filtered_195_1) {
    if ($line =~ /(pattern[0-9]+)/msx) {
    push @grep_matches_195_1, $1;
    }
    }
    $grep_result_195_1 = join "\n", @grep_matches_195_1;
    $CHILD_ERROR = scalar @grep_filtered_195_1 > 0 ? 0 : 1;
    $output_195 = $grep_result_195_1;
    $output_195 = $grep_result_195_1;
    if ((scalar @grep_filtered_195_1) == 0) {
        $pipeline_success_195 = 0;
    }
    if ($output_195 ne q{} && !defined $output_printed_195) {
        print $output_195;
        if (!($output_195 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_195 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

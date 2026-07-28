#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '016_grep_basic.sh';
my $grep_result_176;
my @grep_lines_176 = ();
my @grep_filenames_176 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_176, $line;
        push @grep_filenames_176, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_176 = grep { /pattern/msx } @grep_lines_176;
$grep_result_176 = join "\n", @grep_filtered_176;
if (!($grep_result_176 =~ m{\n\z} || $grep_result_176 eq q{})) {
    $grep_result_176 .= "\n";
}
print $grep_result_176;
$CHILD_ERROR = scalar @grep_filtered_176 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        say "No matches found";
}
# Original bash: echo "HELLO world" | grep -i "hello"
do {
    my $output_177 = q{};
    my $output_printed_177;
    my $pipeline_success_177 = 1;
    $output_177 .= 'HELLO world' . "\n";
if ( !($output_177 =~ m{\n\z}) ) { $output_177 .= "\n"; }

        my $grep_result_177_1;
    my @grep_lines_177_1 = split /\n/msx, $output_177;
    my @grep_filtered_177_1 = grep { /hello/msxi } @grep_lines_177_1;
    $grep_result_177_1 = join "\n", @grep_filtered_177_1;
    if (!($grep_result_177_1 =~ m{\n\z} || $grep_result_177_1 eq q{})) {
    $grep_result_177_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_177_1 > 0 ? 0 : 1;
    $output_177 = $grep_result_177_1;
    $output_177 = $grep_result_177_1;
    if ((scalar @grep_filtered_177_1) == 0) {
        $pipeline_success_177 = 0;
    }
    if ($output_177 ne q{} && !defined $output_printed_177) {
        print $output_177;
        if (!($output_177 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_177 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
do {
    my $output_178 = q{};
    my $output_printed_178;
    my $pipeline_success_178 = 1;
    $output_178 .= "line1\nline2\nline3";
if ( !($output_178 =~ m{\n\z}) ) { $output_178 .= "\n"; }

        my $grep_result_178_1;
    my @grep_lines_178_1 = split /\n/msx, $output_178;
    my @grep_filtered_178_1 = grep { !/line2/msx } @grep_lines_178_1;
    $grep_result_178_1 = join "\n", @grep_filtered_178_1;
    if (!($grep_result_178_1 =~ m{\n\z} || $grep_result_178_1 eq q{})) {
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
        if (!($output_178 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_178 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
do {
    my $output_179 = q{};
    my $output_printed_179;
    my $pipeline_success_179 = 1;
    $output_179 .= "first\nsecond\nthird";
if ( !($output_179 =~ m{\n\z}) ) { $output_179 .= "\n"; }

        my $grep_result_179_1;
    my @grep_lines_179_1 = split /\n/msx, $output_179;
    my @grep_filtered_179_1 = grep { /second/msx } @grep_lines_179_1;
    my @grep_numbered_179_1;
    for my $i (0..@grep_lines_179_1-1) {
    if (scalar grep { $_ eq $grep_lines_179_1[$i] } @grep_filtered_179_1) {
    push @grep_numbered_179_1, sprintf "%d:%s", $i + 1, $grep_lines_179_1[$i];
    }
    }
    $grep_result_179_1 = join "\n", @grep_numbered_179_1;
    $CHILD_ERROR = scalar @grep_filtered_179_1 > 0 ? 0 : 1;
    $output_179 = $grep_result_179_1;
    $output_179 = $grep_result_179_1;
    if ((scalar @grep_filtered_179_1) == 0) {
        $pipeline_success_179 = 0;
    }
    if ($output_179 ne q{} && !defined $output_printed_179) {
        print $output_179;
        if (!($output_179 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_179 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
do {
    my $output_180 = q{};
    my $output_printed_180;
    my $pipeline_success_180 = 1;
    $output_180 .= "match\nno match\nmatch again";
if ( !($output_180 =~ m{\n\z}) ) { $output_180 .= "\n"; }

        my $grep_result_180_1;
    my @grep_lines_180_1 = split /\n/msx, $output_180;
    my @grep_filtered_180_1 = grep { /match/msx } @grep_lines_180_1;
    $grep_result_180_1 = scalar @grep_filtered_180_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_180_1 > 0 ? 0 : 1;
    $output_180 = $grep_result_180_1;
    $output_180 = $grep_result_180_1;
    if ((scalar @grep_filtered_180_1) == 0) {
        $pipeline_success_180 = 0;
    }
    if ($output_180 ne q{} && !defined $output_printed_180) {
        print $output_180;
        if (!($output_180 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_180 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern123 in it" | grep -o "pattern[0-9]\+"
do {
    my $output_181 = q{};
    my $output_printed_181;
    my $pipeline_success_181 = 1;
    $output_181 .= 'text with pattern123 in it' . "\n";
if ( !($output_181 =~ m{\n\z}) ) { $output_181 .= "\n"; }

        my $grep_result_181_1;
    my @grep_lines_181_1 = split /\n/msx, $output_181;
    my @grep_filtered_181_1 = grep { /pattern[0-9]+/msx } @grep_lines_181_1;
    my @grep_matches_181_1;
    foreach my $line (@grep_filtered_181_1) {
    if ($line =~ /(pattern[0-9]+)/msx) {
    push @grep_matches_181_1, $1;
    }
    }
    $grep_result_181_1 = join "\n", @grep_matches_181_1;
    $CHILD_ERROR = scalar @grep_filtered_181_1 > 0 ? 0 : 1;
    $output_181 = $grep_result_181_1;
    $output_181 = $grep_result_181_1;
    if ((scalar @grep_filtered_181_1) == 0) {
        $pipeline_success_181 = 0;
    }
    if ($output_181 ne q{} && !defined $output_printed_181) {
        print $output_181;
        if (!($output_181 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_181 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

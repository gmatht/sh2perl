#!/usr/bin/perl
use strict;
use warnings;

open my $fh_test, '<', 'test_data.txt' or die "Cannot open test_data.txt: $!\n";
local $/ = undef;
my $chunk_test = <$fh_test>;
close $fh_test;

my $output_0 = q{};
my $output_printed_0;
my $pipeline_success_0 = 1;
$output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
my $grep_result_0_1;
my $CHILD_ERROR = 0;
my @grep_lines_0_1 = split /\n/msx, $output_0;
my @grep_filtered_0_1 = grep { /Engineer/msx } @grep_lines_0_1;
$grep_result_0_1 = join "\n", @grep_filtered_0_1;
    if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
        $grep_result_0_1 .= "\n";
    }
$CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
$output_0 = $grep_result_0_1;
if ((scalar @grep_filtered_0_1) == 0) {
    $pipeline_success_0 = 0;
}
my @lines_1 = split /\n/msx, $output_0;
my @result_1;
foreach my $line (@lines_1) {
chomp $line;
my @fields = split /,/msx, $line;
my @sel = ();
if (@fields > 0) { push @sel, $fields[0]; }
if (@fields > 3) { push @sel, $fields[3]; }
push @result_1, join(q{,}, @sel);
}
$output_0 = join "\n", @result_1;

my @sort_lines_0_3 = split /\n/msx, $output_0;
sub sort_numeric_0_3 {
    my @a_fields = split /,/msx, $a;
    my @b_fields = split /,/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    if ( scalar @a_fields > 1 && $a_fields[1] =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_fields[1]; }
    if ( scalar @b_fields > 1 && $b_fields[1] =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_fields[1]; }
    return $a_num <=> $b_num || $a cmp $b;
}
my @sort_sorted_0_3 = sort sort_numeric_0_3 @sort_lines_0_3;
@sort_sorted_0_3 = reverse @sort_sorted_0_3;
$output_0 = join "\n", @sort_sorted_0_3;
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
my $num_lines       = 3;
my $head_line_count = 0;
my $result          = q{};
my $input           = $output_0;
my $pos             = 0;

while ( $pos < length $input && $head_line_count < $num_lines ) {
    my $line_end = index $input, "\n", $pos;
    if ( $line_end == -1 ) {
        $line_end = length $input;
    }
    my $head_line = substr $input, $pos, $line_end - $pos;
    $result .= $head_line . "\n";
    $pos = $line_end + 1;
    ++$head_line_count;
}
$output_0 = $result;

print $output_0;

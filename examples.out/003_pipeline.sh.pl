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

$PROGRAM_NAME = '003_pipeline.sh';
# Original bash: ls | grep "\.txt$" | wc -l
{
    my $output_137 = q{};
    my $output_printed_137;
    my $pipeline_success_137 = 1;
        $output_137 = do {
    my @ls_files_138 = ();
    if ( -f q{.} ) {
    push @ls_files_138, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_138, $file;
    }
    closedir $dh;
    @ls_files_138 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_138;
    }
    }
    (@ls_files_138 ? join("\n", @ls_files_138) . "\n" : q{});
    };
    ;

        my $grep_result_137_1;
    my @grep_lines_137_1 = split /\n/msx, $output_137;
    my @grep_filtered_137_1 = grep { /[.]txt$/msx } @grep_lines_137_1;
    $grep_result_137_1 = join "\n", @grep_filtered_137_1;
    if (!($grep_result_137_1 =~ m{\n\z}msx || $grep_result_137_1 eq q{})) {
    $grep_result_137_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_137_1 > 0 ? 0 : 1;
    $output_137 = $grep_result_137_1;
    $output_137 = $grep_result_137_1;

        my $output_137_2 = do {
    my $_wc_data = $output_137;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_lines;
    $_wc_result .= "\n";
    $_wc_result;
    };
    $output_137 = $output_137_2;
    if ($output_137 ne q{} && !defined $output_printed_137) {
        print $output_137;
        if (!($output_137 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_137 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_140 = q{};
    my $output_printed_140;
    my $pipeline_success_140 = 1;
        $output_140 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_140_1 = split /\n/msx, $output_140;
    my @sort_sorted_140_1 = sort @sort_lines_140_1;
    my $output_140_1 = join "\n", @sort_sorted_140_1;
    if ($output_140_1 ne q{} && !($output_140_1 =~ m{\n\z}msx)) {
    $output_140_1 .= "\n";
    }
    $output_140 = $output_140_1;
    $output_140 = $output_140_1;

        my @uniq_lines_140_2 = split /\n/msx, $output_140;
    @uniq_lines_140_2 = grep { $_ ne q{} } @uniq_lines_140_2; # Filter out empty lines
    my %uniq_counts_140_2;
    my @uniq_order_140_2;
    foreach my $line (@uniq_lines_140_2) {
    if (!exists $uniq_counts_140_2{$line}) { push @uniq_order_140_2, $line; }
    $uniq_counts_140_2{$line}++;
    }
    my @uniq_result_140_2;
    foreach my $line (@uniq_order_140_2) {
    push @uniq_result_140_2, sprintf "%7d %s", $uniq_counts_140_2{$line}, $line;
    }
    my $output_140_2 = join "\n", @uniq_result_140_2;
    if ($output_140_2 ne q{} && !($output_140_2 =~ m{\n\z}msx)) {
    $output_140_2 .= "\n";
    }
    $output_140 = $output_140_2;

        my @sort_lines_140_3 = split /\n/msx, $output_140;
    my @sort_sorted_140_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_140_3;
    @sort_sorted_140_3 = reverse @sort_sorted_140_3;
    my $output_140_3 = join "\n", @sort_sorted_140_3;
    if ($output_140_3 ne q{} && !($output_140_3 =~ m{\n\z}msx)) {
    $output_140_3 .= "\n";
    }
    $output_140 = $output_140_3;
    $output_140 = $output_140_3;
    if ($output_140 ne q{} && !defined $output_printed_140) {
        print $output_140;
        if (!($output_140 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_140 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_141 = q{};
    my $output_printed_141;
    my $pipeline_success_141 = 1;
        $output_141 = do {
    require File::Find;
    my @find_results;
    File::Find::find(sub { if ($_ =~ /^.*\.sh$/msx) { push @find_results, $File::Find::name; } }, q{.});
    my $result = join "\n", @find_results;
    if ($result ne q{}) { $result .= "\n"; }
    $CHILD_ERROR = 0;
    $result;
    };

        my @xargs_files_141_1 = split /\n/msx, $output_141;
    my @xargs_matching_files_141_1;
    foreach my $file (@xargs_files_141_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_141_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_141_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_141_1) { push @xargs_matching_files_141_1, $file; }
    }
    }
    my $xargs_result_141_1 = join "\n", @xargs_matching_files_141_1;
    if (!($xargs_result_141_1 =~ m{\n\z}msx)) {
    $xargs_result_141_1 .= "\n";
    }
    $output_141 = $xargs_result_141_1;

        my $set1_142 = "\\/";
    my $input_142 = $output_141;
    my $tr_result_141_2 = q{};
    for my $char ( split //msx, $input_142 ) {
    if ( (index $set1_142, $char) == -1 ) {
    $tr_result_141_2 .= $char;
    }
    }
    if (!($tr_result_141_2 =~ m{\n\z}msx || $tr_result_141_2 eq q{})) {
    $tr_result_141_2 .= "\n";
    }
    $output_141 = $tr_result_141_2;
    $output_141 = $tr_result_141_2;
    if ($output_141 ne q{} && !defined $output_printed_141) {
        print $output_141;
        if (!($output_141 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_141 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_143 = q{};
    my $output_printed_143;
    my $pipeline_success_143 = 1;
        $output_143 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my $set1_144 = q{a};
    my $set2_144 = q{b};
    my $input_144 = $output_143;
    # Expand character ranges for tr command
    my $expanded_set1_144 = $set1_144;
    my $expanded_set2_144 = $set2_144;
    # Handle a-z range in set1
    if ($expanded_set1_144 =~ /a-z/msx) {
    $expanded_set1_144 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_144 =~ /A-Z/msx) {
    $expanded_set1_144 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_144 =~ /\[:upper:\]/msx) {
    $expanded_set1_144 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_144 =~ /\[:lower:\]/msx) {
    $expanded_set1_144 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_144 =~ /a-z/msx) {
    $expanded_set2_144 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_144 =~ /A-Z/msx) {
    $expanded_set2_144 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_144 =~ /\[:upper:\]/msx) {
    $expanded_set2_144 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_144 =~ /\[:lower:\]/msx) {
    $expanded_set2_144 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_143_1 = q{};
    for my $char ( split //msx, $input_144 ) {
    my $pos_144 = index $expanded_set1_144, $char;
    if ( $pos_144 >= 0 && $pos_144 < length $expanded_set2_144 ) {
    $tr_result_143_1 .= substr $expanded_set2_144, $pos_144, 1;
    } else {
    $tr_result_143_1 .= $char;
    }
    }
    if (!($tr_result_143_1 =~ m{\n\z}msx || $tr_result_143_1 eq q{})) {
    $tr_result_143_1 .= "\n";
    }
    $output_143 = $tr_result_143_1;
    $output_143 = $tr_result_143_1;

        my $grep_result_143_2;
    my @grep_lines_143_2 = split /\n/msx, $output_143;
    my @grep_filtered_143_2 = grep { /hello/msx } @grep_lines_143_2;
    $grep_result_143_2 = join "\n", @grep_filtered_143_2;
    if (!($grep_result_143_2 =~ m{\n\z}msx || $grep_result_143_2 eq q{})) {
    $grep_result_143_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_143_2 > 0 ? 0 : 1;
    $output_143 = $grep_result_143_2;
    $output_143 = $grep_result_143_2;
    if ((scalar @grep_filtered_143_2) == 0) {
        $pipeline_success_143 = 0;
    }
    if ($output_143 ne q{} && !defined $output_printed_143) {
        print $output_143;
        if (!($output_143 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_143 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
{
    my $output_145 = q{};
    my $output_printed_145;
    my $pipeline_success_145 = 1;
        $output_145 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_145_1 = split /\n/msx, $output_145;
    my @sort_sorted_145_1 = sort @sort_lines_145_1;
    my $output_145_1 = join "\n", @sort_sorted_145_1;
    if ($output_145_1 ne q{} && !($output_145_1 =~ m{\n\z}msx)) {
    $output_145_1 .= "\n";
    }
    $output_145 = $output_145_1;
    $output_145 = $output_145_1;

        my $grep_result_145_2;
    my @grep_lines_145_2 = split /\n/msx, $output_145;
    my @grep_filtered_145_2 = grep { /hello/msx } @grep_lines_145_2;
    $grep_result_145_2 = join "\n", @grep_filtered_145_2;
    if (!($grep_result_145_2 =~ m{\n\z}msx || $grep_result_145_2 eq q{})) {
    $grep_result_145_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_145_2 > 0 ? 0 : 1;
    $output_145 = $grep_result_145_2;
    $output_145 = $grep_result_145_2;
    if ((scalar @grep_filtered_145_2) == 0) {
        $pipeline_success_145 = 0;
    }
    if ($output_145 ne q{} && !defined $output_printed_145) {
        print $output_145;
        if (!($output_145 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_145 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

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

$PROGRAM_NAME = '003_pipeline.sh';
# Original bash: ls | grep "\.txt$" | wc -l
{
    my $output_136 = q{};
    my $output_printed_136;
    my $pipeline_success_136 = 1;
        $output_136 = do {
    my @ls_files_137 = ();
    if ( -f q{.} ) {
    push @ls_files_137, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_137, $file;
    }
    closedir $dh;
    @ls_files_137 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_137;
    }
    }
    (@ls_files_137 ? join("\n", @ls_files_137) . "\n" : q{});
    };
    ;

        my $grep_result_136_1;
    my @grep_lines_136_1 = split /\n/msx, $output_136;
    my @grep_filtered_136_1 = grep { /[.]txt$/msx } @grep_lines_136_1;
    $grep_result_136_1 = join "\n", @grep_filtered_136_1;
    if (!($grep_result_136_1 =~ m{\n\z}msx || $grep_result_136_1 eq q{})) {
    $grep_result_136_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_136_1 > 0 ? 0 : 1;
    $output_136 = $grep_result_136_1;
    $output_136 = $grep_result_136_1;

        my $output_136_2 = do {
    my $_wc_data = $output_136;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_lines;
    $_wc_result .= "\n";
    $_wc_result;
    };
    $output_136 = $output_136_2;
    if ($output_136 ne q{} && !defined $output_printed_136) {
        print $output_136;
        if (!($output_136 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_136 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_139 = q{};
    my $output_printed_139;
    my $pipeline_success_139 = 1;
        $output_139 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_139_1 = split /\n/msx, $output_139;
    my @sort_sorted_139_1 = sort @sort_lines_139_1;
    my $output_139_1 = join "\n", @sort_sorted_139_1;
    if ($output_139_1 ne q{} && !($output_139_1 =~ m{\n\z}msx)) {
    $output_139_1 .= "\n";
    }
    $output_139 = $output_139_1;
    $output_139 = $output_139_1;

        my @uniq_lines_139_2 = split /\n/msx, $output_139;
    @uniq_lines_139_2 = grep { $_ ne q{} } @uniq_lines_139_2; # Filter out empty lines
    my %uniq_counts_139_2;
    my @uniq_order_139_2;
    foreach my $line (@uniq_lines_139_2) {
    if (!exists $uniq_counts_139_2{$line}) { push @uniq_order_139_2, $line; }
    $uniq_counts_139_2{$line}++;
    }
    my @uniq_result_139_2;
    foreach my $line (@uniq_order_139_2) {
    push @uniq_result_139_2, sprintf "%7d %s", $uniq_counts_139_2{$line}, $line;
    }
    my $output_139_2 = join "\n", @uniq_result_139_2;
    if ($output_139_2 ne q{} && !($output_139_2 =~ m{\n\z}msx)) {
    $output_139_2 .= "\n";
    }
    $output_139 = $output_139_2;

        my @sort_lines_139_3 = split /\n/msx, $output_139;
    my @sort_sorted_139_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_139_3;
    @sort_sorted_139_3 = reverse @sort_sorted_139_3;
    my $output_139_3 = join "\n", @sort_sorted_139_3;
    if ($output_139_3 ne q{} && !($output_139_3 =~ m{\n\z}msx)) {
    $output_139_3 .= "\n";
    }
    $output_139 = $output_139_3;
    $output_139 = $output_139_3;
    if ($output_139 ne q{} && !defined $output_printed_139) {
        print $output_139;
        if (!($output_139 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_139 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_140 = q{};
    my $output_printed_140;
    my $pipeline_success_140 = 1;
        $output_140 = do { my $command = q{find . -name '*.sh'}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };

        my @xargs_files_140_1 = split /\n/msx, $output_140;
    my @xargs_matching_files_140_1;
    foreach my $file (@xargs_files_140_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_140_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_140_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_140_1) { push @xargs_matching_files_140_1, $file; }
    }
    }
    my $xargs_result_140_1 = join "\n", @xargs_matching_files_140_1;
    if (!($xargs_result_140_1 =~ m{\n\z}msx)) {
    $xargs_result_140_1 .= "\n";
    }
    $output_140 = $xargs_result_140_1;

        my $set1_141 = "\\/";
    my $input_141 = $output_140;
    my $tr_result_140_2 = q{};
    for my $char ( split //msx, $input_141 ) {
    if ( (index $set1_141, $char) == -1 ) {
    $tr_result_140_2 .= $char;
    }
    }
    if (!($tr_result_140_2 =~ m{\n\z}msx || $tr_result_140_2 eq q{})) {
    $tr_result_140_2 .= "\n";
    }
    $output_140 = $tr_result_140_2;
    $output_140 = $tr_result_140_2;
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
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_142 = q{};
    my $output_printed_142;
    my $pipeline_success_142 = 1;
        $output_142 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my $set1_143 = q{a};
    my $set2_143 = q{b};
    my $input_143 = $output_142;
    # Expand character ranges for tr command
    my $expanded_set1_143 = $set1_143;
    my $expanded_set2_143 = $set2_143;
    # Handle a-z range in set1
    if ($expanded_set1_143 =~ /a-z/msx) {
    $expanded_set1_143 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_143 =~ /A-Z/msx) {
    $expanded_set1_143 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_143 =~ /\[:upper:\]/msx) {
    $expanded_set1_143 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_143 =~ /\[:lower:\]/msx) {
    $expanded_set1_143 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_143 =~ /a-z/msx) {
    $expanded_set2_143 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_143 =~ /A-Z/msx) {
    $expanded_set2_143 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_143 =~ /\[:upper:\]/msx) {
    $expanded_set2_143 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_143 =~ /\[:lower:\]/msx) {
    $expanded_set2_143 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_142_1 = q{};
    for my $char ( split //msx, $input_143 ) {
    my $pos_143 = index $expanded_set1_143, $char;
    if ( $pos_143 >= 0 && $pos_143 < length $expanded_set2_143 ) {
    $tr_result_142_1 .= substr $expanded_set2_143, $pos_143, 1;
    } else {
    $tr_result_142_1 .= $char;
    }
    }
    if (!($tr_result_142_1 =~ m{\n\z}msx || $tr_result_142_1 eq q{})) {
    $tr_result_142_1 .= "\n";
    }
    $output_142 = $tr_result_142_1;
    $output_142 = $tr_result_142_1;

        my $grep_result_142_2;
    my @grep_lines_142_2 = split /\n/msx, $output_142;
    my @grep_filtered_142_2 = grep { /hello/msx } @grep_lines_142_2;
    $grep_result_142_2 = join "\n", @grep_filtered_142_2;
    if (!($grep_result_142_2 =~ m{\n\z}msx || $grep_result_142_2 eq q{})) {
    $grep_result_142_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_142_2 > 0 ? 0 : 1;
    $output_142 = $grep_result_142_2;
    $output_142 = $grep_result_142_2;
    if ((scalar @grep_filtered_142_2) == 0) {
        $pipeline_success_142 = 0;
    }
    if ($output_142 ne q{} && !defined $output_printed_142) {
        print $output_142;
        if (!($output_142 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_142 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
{
    my $output_144 = q{};
    my $output_printed_144;
    my $pipeline_success_144 = 1;
        $output_144 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_144_1 = split /\n/msx, $output_144;
    my @sort_sorted_144_1 = sort @sort_lines_144_1;
    my $output_144_1 = join "\n", @sort_sorted_144_1;
    if ($output_144_1 ne q{} && !($output_144_1 =~ m{\n\z}msx)) {
    $output_144_1 .= "\n";
    }
    $output_144 = $output_144_1;
    $output_144 = $output_144_1;

        my $grep_result_144_2;
    my @grep_lines_144_2 = split /\n/msx, $output_144;
    my @grep_filtered_144_2 = grep { /hello/msx } @grep_lines_144_2;
    $grep_result_144_2 = join "\n", @grep_filtered_144_2;
    if (!($grep_result_144_2 =~ m{\n\z}msx || $grep_result_144_2 eq q{})) {
    $grep_result_144_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_144_2 > 0 ? 0 : 1;
    $output_144 = $grep_result_144_2;
    $output_144 = $grep_result_144_2;
    if ((scalar @grep_filtered_144_2) == 0) {
        $pipeline_success_144 = 0;
    }
    if ($output_144 ne q{} && !defined $output_printed_144) {
        print $output_144;
        if (!($output_144 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_144 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

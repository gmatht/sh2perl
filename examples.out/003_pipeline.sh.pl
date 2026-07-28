#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '003_pipeline.sh';
# Original bash: ls | grep "\.txt$" | wc -l
do {
    my $output_127 = q{};
    my $output_printed_127;
    my $pipeline_success_127 = 1;
        $output_127 = do {
    my @ls_files_128 = ();
    if ( -f q{.} ) {
    push @ls_files_128, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
    push @ls_files_128, $file;
    }
    closedir $dh;
    @ls_files_128 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_files_128;
    }
    }
    (@ls_files_128 ? join("\n", @ls_files_128) . "\n" : q{});
    };
    ;

        my $grep_result_127_1;
    my @grep_lines_127_1 = split /\n/msx, $output_127;
    my @grep_filtered_127_1 = grep { /[.]txt$/msx } @grep_lines_127_1;
    $grep_result_127_1 = join "\n", @grep_filtered_127_1;
    if (!($grep_result_127_1 =~ m{\n\z} || $grep_result_127_1 eq q{})) {
    $grep_result_127_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_127_1 > 0 ? 0 : 1;
    $output_127 = $grep_result_127_1;
    $output_127 = $grep_result_127_1;

        my $output_127_2 = do {
    my $_wc_data = $output_127;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = sprintf("%d \n", $_wc_lines);
    $_wc_result;
    };
    $output_127 = $output_127_2;
    if ($output_127 ne q{} && !defined $output_printed_127) {
        print $output_127;
        if (!($output_127 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_127 ) { $main_exit_code = 1; }
    }
print "\n";
# Original bash: cat file.txt | sort | uniq -c | sort -nr
do {
    my $output_130 = q{};
    my $output_printed_130;
    my $pipeline_success_130 = 1;
        $output_130 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_130_1 = split /\n/, $output_130;
    my @sort_sorted_130_1 = sort @sort_lines_130_1;
    my $output_130_1 = join("\n", @sort_sorted_130_1);
    $output_130 = $output_130_1;
    $output_130 = $output_130_1;

        my @uniq_lines_130_2 = split /\n/, $output_130;
    @uniq_lines_130_2 = grep { $_ ne q{} } @uniq_lines_130_2; # Filter out empty lines
    my %uniq_counts_130_2;
    my @uniq_order_130_2;
    foreach my $line (@uniq_lines_130_2) {
    if (!exists $uniq_counts_130_2{$line}) { push @uniq_order_130_2, $line; }
    $uniq_counts_130_2{$line}++;
    }
    my @uniq_result_130_2;
    foreach my $line (@uniq_order_130_2) {
    push @uniq_result_130_2, sprintf "%7d %s", $uniq_counts_130_2{$line}, $line;
    }
    my $output_130_2 = join "\n", @uniq_result_130_2;
    if ($output_130_2 ne q{} && !($output_130_2 =~ m{\n\z})) {
    $output_130_2 .= "\n";
    }
    $output_130 = $output_130_2;

        my @sort_lines_130_3 = split /\n/, $output_130;
    my @sort_sorted_130_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_130_3;
    @sort_sorted_130_3 = reverse @sort_sorted_130_3;
    my $output_130_3 = join("\n", @sort_sorted_130_3);
    $output_130 = $output_130_3;
    $output_130 = $output_130_3;
    if ($output_130 ne q{} && !defined $output_printed_130) {
        print $output_130;
        if (!($output_130 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_130 ) { $main_exit_code = 1; }
    }
print "\n";
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
do {
    my $output_131 = q{};
    my $output_printed_131;
    my $pipeline_success_131 = 1;
        $output_131 = do {
    require File::Find;
    my @find_results;
    File::Find::find(sub { if ($_ =~ /^.*\.sh$/) { push @find_results, $File::Find::name; } }, q{.});
    my $result = join "\n", @find_results;
    if ($result ne q{}) { $result .= "\n"; }
    $CHILD_ERROR = 0;
    $result;
    };

        my @xargs_files_131_1 = split /\n/, $output_131;
    my @xargs_matching_files_131_1;
    foreach my $file (@xargs_files_131_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_131_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_131_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_131_1) { push @xargs_matching_files_131_1, $file; }
    }
    }
    my $xargs_result_131_1 = join "\n", @xargs_matching_files_131_1;
    if (!($xargs_result_131_1 =~ m{\n\z})) {
    $xargs_result_131_1 .= "\n";
    }
    $output_131 = $xargs_result_131_1;

        my $set1_132 = "\\/";
    my $input_132 = $output_131;
    my $tr_result_131_2 = q{};
    for my $char ( split //msx, $input_132 ) {
    if ( (index $set1_132, $char) == -1 ) {
    $tr_result_131_2 .= $char;
    }
    }
    if (!($tr_result_131_2 =~ m{\n\z} || $tr_result_131_2 eq q{})) {
    $tr_result_131_2 .= "\n";
    }
    $output_131 = $tr_result_131_2;
    $output_131 = $tr_result_131_2;
    if ($output_131 ne q{} && !defined $output_printed_131) {
        print $output_131;
        if (!($output_131 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_131 ) { $main_exit_code = 1; }
    }
print "\n";
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
do {
    my $output_133 = q{};
    my $output_printed_133;
    my $pipeline_success_133 = 1;
        $output_133 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my $set1_134 = q{a};
    my $set2_134 = q{b};
    my $input_134 = $output_133;
    # Expand character ranges for tr command
    my $expanded_set1_134 = $set1_134;
    my $expanded_set2_134 = $set2_134;
    # Handle a-z range in set1
    if ($expanded_set1_134 =~ /a-z/msx) {
    $expanded_set1_134 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_134 =~ /A-Z/msx) {
    $expanded_set1_134 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_134 =~ /\[:upper:\]/msx) {
    $expanded_set1_134 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_134 =~ /\[:lower:\]/msx) {
    $expanded_set1_134 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_134 =~ /a-z/msx) {
    $expanded_set2_134 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_134 =~ /A-Z/msx) {
    $expanded_set2_134 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_134 =~ /\[:upper:\]/msx) {
    $expanded_set2_134 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_134 =~ /\[:lower:\]/msx) {
    $expanded_set2_134 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_133_1 = q{};
    for my $char ( split //msx, $input_134 ) {
    my $pos_134 = index $expanded_set1_134, $char;
    if ( $pos_134 >= 0 && $pos_134 < length $expanded_set2_134 ) {
    $tr_result_133_1 .= substr $expanded_set2_134, $pos_134, 1;
    } else {
    $tr_result_133_1 .= $char;
    }
    }
    if (!($tr_result_133_1 =~ m{\n\z} || $tr_result_133_1 eq q{})) {
    $tr_result_133_1 .= "\n";
    }
    $output_133 = $tr_result_133_1;
    $output_133 = $tr_result_133_1;

        my $grep_result_133_2;
    my @grep_lines_133_2 = split /\n/msx, $output_133;
    my @grep_filtered_133_2 = grep { /hello/msx } @grep_lines_133_2;
    $grep_result_133_2 = join "\n", @grep_filtered_133_2;
    if (!($grep_result_133_2 =~ m{\n\z} || $grep_result_133_2 eq q{})) {
    $grep_result_133_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_133_2 > 0 ? 0 : 1;
    $output_133 = $grep_result_133_2;
    $output_133 = $grep_result_133_2;
    if ((scalar @grep_filtered_133_2) == 0) {
        $pipeline_success_133 = 0;
    }
    if ($output_133 ne q{} && !defined $output_printed_133) {
        print $output_133;
        if (!($output_133 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_133 ) { $main_exit_code = 1; }
    }
print "\n";
do {
    my $output_135 = q{};
    my $output_printed_135;
    my $pipeline_success_135 = 1;
        $output_135 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_135_1 = split /\n/, $output_135;
    my @sort_sorted_135_1 = sort @sort_lines_135_1;
    my $output_135_1 = join("\n", @sort_sorted_135_1);
    $output_135 = $output_135_1;
    $output_135 = $output_135_1;

        my $grep_result_135_2;
    my @grep_lines_135_2 = split /\n/msx, $output_135;
    my @grep_filtered_135_2 = grep { /hello/msx } @grep_lines_135_2;
    $grep_result_135_2 = join "\n", @grep_filtered_135_2;
    if (!($grep_result_135_2 =~ m{\n\z} || $grep_result_135_2 eq q{})) {
    $grep_result_135_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_135_2 > 0 ? 0 : 1;
    $output_135 = $grep_result_135_2;
    $output_135 = $grep_result_135_2;
    if ((scalar @grep_filtered_135_2) == 0) {
        $pipeline_success_135 = 0;
    }
    if ($output_135 ne q{} && !defined $output_printed_135) {
        print $output_135;
        if (!($output_135 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_135 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

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
    my $output_134 = q{};
    my $output_printed_134;
    my $pipeline_success_134 = 1;
        $output_134 = do {
    my @ls_files_135 = ();
    if ( -f q{.} ) {
    push @ls_files_135, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_135, $file;
    }
    closedir $dh;
    @ls_files_135 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_135;
    }
    }
    (@ls_files_135 ? join("\n", @ls_files_135) . "\n" : q{});
    };
    ;

        my $grep_result_134_1;
    my @grep_lines_134_1 = split /\n/msx, $output_134;
    my @grep_filtered_134_1 = grep { /[.]txt$/msx } @grep_lines_134_1;
    $grep_result_134_1 = join "\n", @grep_filtered_134_1;
    if (!($grep_result_134_1 =~ m{\n\z}msx || $grep_result_134_1 eq q{})) {
    $grep_result_134_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_134_1 > 0 ? 0 : 1;
    $output_134 = $grep_result_134_1;
    $output_134 = $grep_result_134_1;

        use IPC::Open3;
    my @wc_args_134_2 = ('-l');
    my ($wc_in_134_2, $wc_out_134_2, $wc_err_134_2);
    my $wc_pid_134_2 = open3($wc_in_134_2, $wc_out_134_2, $wc_err_134_2, 'wc', @wc_args_134_2);
    print {$wc_in_134_2} $output_134;
    close $wc_in_134_2 or die "Close failed: $OS_ERROR\n";
    my $output_134_2 = do { local $/ = undef; <$wc_out_134_2> };
    if ($output_134_2 eq q{}) { $output_134_2 = "0\n"; }
    close $wc_out_134_2 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_134_2, 0;
    $output_134 = $output_134_2;
    if ($output_134 ne q{} && !defined $output_printed_134) {
        print $output_134;
        if (!($output_134 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_134 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_137 = q{};
    my $output_printed_137;
    my $pipeline_success_137 = 1;
        $output_137 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_137_1 = split /\n/msx, $output_137;
    my @sort_sorted_137_1 = sort @sort_lines_137_1;
    my $output_137_1 = join "\n", @sort_sorted_137_1;
    if ($output_137_1 ne q{} && !($output_137_1 =~ m{\n\z}msx)) {
    $output_137_1 .= "\n";
    }
    $output_137 = $output_137_1;
    $output_137 = $output_137_1;

        my @uniq_lines_137_2 = split /\n/msx, $output_137;
    @uniq_lines_137_2 = grep { $_ ne q{} } @uniq_lines_137_2; # Filter out empty lines
    my %uniq_counts_137_2;
    my @uniq_order_137_2;
    foreach my $line (@uniq_lines_137_2) {
    if (!exists $uniq_counts_137_2{$line}) { push @uniq_order_137_2, $line; }
    $uniq_counts_137_2{$line}++;
    }
    my @uniq_result_137_2;
    foreach my $line (@uniq_order_137_2) {
    push @uniq_result_137_2, sprintf "%7d %s", $uniq_counts_137_2{$line}, $line;
    }
    my $output_137_2 = join "\n", @uniq_result_137_2;
    if ($output_137_2 ne q{} && !($output_137_2 =~ m{\n\z}msx)) {
    $output_137_2 .= "\n";
    }
    $output_137 = $output_137_2;

        my @sort_lines_137_3 = split /\n/msx, $output_137;
    my @sort_sorted_137_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_137_3;
    @sort_sorted_137_3 = reverse @sort_sorted_137_3;
    my $output_137_3 = join "\n", @sort_sorted_137_3;
    if ($output_137_3 ne q{} && !($output_137_3 =~ m{\n\z}msx)) {
    $output_137_3 .= "\n";
    }
    $output_137 = $output_137_3;
    $output_137 = $output_137_3;
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
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_138 = q{};
    my $output_printed_138;
    my $pipeline_success_138 = 1;
        $output_138 = do { my $command = q{find . -name '*.sh'}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };

        my @xargs_files_138_1 = split /\n/msx, $output_138;
    my @xargs_matching_files_138_1;
    foreach my $file (@xargs_files_138_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_138_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_138_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_138_1) { push @xargs_matching_files_138_1, $file; }
    }
    }
    my $xargs_result_138_1 = join "\n", @xargs_matching_files_138_1;
    if (!($xargs_result_138_1 =~ m{\n\z}msx)) {
    $xargs_result_138_1 .= "\n";
    }
    $output_138 = $xargs_result_138_1;

        my $set1_139 = "\\/";
    my $input_139 = $output_138;
    my $tr_result_138_2 = q{};
    for my $char ( split //msx, $input_139 ) {
    if ( (index $set1_139, $char) == -1 ) {
    $tr_result_138_2 .= $char;
    }
    }
    if (!($tr_result_138_2 =~ m{\n\z}msx || $tr_result_138_2 eq q{})) {
    $tr_result_138_2 .= "\n";
    }
    $output_138 = $tr_result_138_2;
    $output_138 = $tr_result_138_2;
    if ($output_138 ne q{} && !defined $output_printed_138) {
        print $output_138;
        if (!($output_138 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_138 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_140 = q{};
    my $output_printed_140;
    my $pipeline_success_140 = 1;
        $output_140 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my $set1_141 = q{a};
    my $set2_141 = q{b};
    my $input_141 = $output_140;
    # Expand character ranges for tr command
    my $expanded_set1_141 = $set1_141;
    my $expanded_set2_141 = $set2_141;
    # Handle a-z range in set1
    if ($expanded_set1_141 =~ /a-z/msx) {
    $expanded_set1_141 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_141 =~ /A-Z/msx) {
    $expanded_set1_141 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_141 =~ /a-z/msx) {
    $expanded_set2_141 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_141 =~ /A-Z/msx) {
    $expanded_set2_141 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_140_1 = q{};
    for my $char ( split //msx, $input_141 ) {
    my $pos_141 = index $expanded_set1_141, $char;
    if ( $pos_141 >= 0 && $pos_141 < length $expanded_set2_141 ) {
    $tr_result_140_1 .= substr $expanded_set2_141, $pos_141, 1;
    } else {
    $tr_result_140_1 .= $char;
    }
    }
    if (!($tr_result_140_1 =~ m{\n\z}msx || $tr_result_140_1 eq q{})) {
    $tr_result_140_1 .= "\n";
    }
    $output_140 = $tr_result_140_1;
    $output_140 = $tr_result_140_1;

        my $grep_result_140_2;
    my @grep_lines_140_2 = split /\n/msx, $output_140;
    my @grep_filtered_140_2 = grep { /hello/msx } @grep_lines_140_2;
    $grep_result_140_2 = join "\n", @grep_filtered_140_2;
    if (!($grep_result_140_2 =~ m{\n\z}msx || $grep_result_140_2 eq q{})) {
    $grep_result_140_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_140_2 > 0 ? 0 : 1;
    $output_140 = $grep_result_140_2;
    $output_140 = $grep_result_140_2;
    if ((scalar @grep_filtered_140_2) == 0) {
        $pipeline_success_140 = 0;
    }
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
{
    my $output_142 = q{};
    my $output_printed_142;
    my $pipeline_success_142 = 1;
        $output_142 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_142_1 = split /\n/msx, $output_142;
    my @sort_sorted_142_1 = sort @sort_lines_142_1;
    my $output_142_1 = join "\n", @sort_sorted_142_1;
    if ($output_142_1 ne q{} && !($output_142_1 =~ m{\n\z}msx)) {
    $output_142_1 .= "\n";
    }
    $output_142 = $output_142_1;
    $output_142 = $output_142_1;

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

exit $main_exit_code;

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
    my $output_131 = q{};
    my $output_printed_131;
    my $pipeline_success_131 = 1;
        $output_131 = do {
    my @ls_files_132 = ();
    if ( -f q{.} ) {
    push @ls_files_132, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_132, $file;
    }
    closedir $dh;
    @ls_files_132 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_132;
    }
    }
    (@ls_files_132 ? join("\n", @ls_files_132) . "\n" : q{});
    };
    ;

        my $grep_result_131_1;
    my @grep_lines_131_1 = split /\n/msx, $output_131;
    my @grep_filtered_131_1 = grep { /[.]txt$/msx } @grep_lines_131_1;
    $grep_result_131_1 = join "\n", @grep_filtered_131_1;
    if (!($grep_result_131_1 =~ m{\n\z}msx || $grep_result_131_1 eq q{})) {
    $grep_result_131_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_131_1 > 0 ? 0 : 1;
    $output_131 = $grep_result_131_1;
    $output_131 = $grep_result_131_1;

        use IPC::Open3;
    my @wc_args_131_2 = ('-l');
    my ($wc_in_131_2, $wc_out_131_2, $wc_err_131_2);
    my $wc_pid_131_2 = open3($wc_in_131_2, $wc_out_131_2, $wc_err_131_2, 'wc', @wc_args_131_2);
    print {$wc_in_131_2} $output_131;
    close $wc_in_131_2 or die "Close failed: $OS_ERROR\n";
    my $output_131_2 = do { local $/ = undef; <$wc_out_131_2> };
    if ($output_131_2 eq q{}) { $output_131_2 = "0\n"; }
    close $wc_out_131_2 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_131_2, 0;
    $output_131 = $output_131_2;
    if ($output_131 ne q{} && !defined $output_printed_131) {
        print $output_131;
        if (!($output_131 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_131 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_134 = q{};
    my $output_printed_134;
    my $pipeline_success_134 = 1;
        $output_134 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my @sort_lines_134_1 = split /\n/msx, $output_134;
    my @sort_sorted_134_1 = sort @sort_lines_134_1;
    my $output_134_1 = join "\n", @sort_sorted_134_1;
    if ($output_134_1 ne q{} && !($output_134_1 =~ m{\n\z}msx)) {
    $output_134_1 .= "\n";
    }
    $output_134 = $output_134_1;
    $output_134 = $output_134_1;

        my @uniq_lines_134_2 = split /\n/msx, $output_134;
    @uniq_lines_134_2 = grep { $_ ne q{} } @uniq_lines_134_2; # Filter out empty lines
    my %uniq_counts_134_2;
    my @uniq_order_134_2;
    foreach my $line (@uniq_lines_134_2) {
    if (!exists $uniq_counts_134_2{$line}) { push @uniq_order_134_2, $line; }
    $uniq_counts_134_2{$line}++;
    }
    my @uniq_result_134_2;
    foreach my $line (@uniq_order_134_2) {
    push @uniq_result_134_2, sprintf "%7d %s", $uniq_counts_134_2{$line}, $line;
    }
    my $output_134_2 = join "\n", @uniq_result_134_2;
    if ($output_134_2 ne q{} && !($output_134_2 =~ m{\n\z}msx)) {
    $output_134_2 .= "\n";
    }
    $output_134 = $output_134_2;

        my @sort_lines_134_3 = split /\n/msx, $output_134;
    my @sort_sorted_134_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_134_3;
    @sort_sorted_134_3 = reverse @sort_sorted_134_3;
    my $output_134_3 = join "\n", @sort_sorted_134_3;
    if ($output_134_3 ne q{} && !($output_134_3 =~ m{\n\z}msx)) {
    $output_134_3 .= "\n";
    }
    $output_134 = $output_134_3;
    $output_134 = $output_134_3;
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
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_135 = q{};
    my $output_printed_135;
    my $pipeline_success_135 = 1;
        $output_135 = do { my $command = q{find . -name '*.sh'}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };

        my @xargs_files_135_1 = split /\n/msx, $output_135;
    my @xargs_matching_files_135_1;
    foreach my $file (@xargs_files_135_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_135_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_135_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_135_1) { push @xargs_matching_files_135_1, $file; }
    }
    }
    my $xargs_result_135_1 = join "\n", @xargs_matching_files_135_1;
    if (!($xargs_result_135_1 =~ m{\n\z}msx)) {
    $xargs_result_135_1 .= "\n";
    }
    $output_135 = $xargs_result_135_1;

        my $set1_136 = "\\/";
    my $input_136 = $output_135;
    my $tr_result_135_2 = q{};
    for my $char ( split //msx, $input_136 ) {
    if ( (index $set1_136, $char) == -1 ) {
    $tr_result_135_2 .= $char;
    }
    }
    if (!($tr_result_135_2 =~ m{\n\z}msx || $tr_result_135_2 eq q{})) {
    $tr_result_135_2 .= "\n";
    }
    $output_135 = $tr_result_135_2;
    $output_135 = $tr_result_135_2;
    if ($output_135 ne q{} && !defined $output_printed_135) {
        print $output_135;
        if (!($output_135 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_135 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_137 = q{};
    my $output_printed_137;
    my $pipeline_success_137 = 1;
        $output_137 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };

        my $set1_138 = q{a};
    my $set2_138 = q{b};
    my $input_138 = $output_137;
    # Expand character ranges for tr command
    my $expanded_set1_138 = $set1_138;
    my $expanded_set2_138 = $set2_138;
    # Handle a-z range in set1
    if ($expanded_set1_138 =~ /a-z/msx) {
    $expanded_set1_138 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_138 =~ /A-Z/msx) {
    $expanded_set1_138 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_138 =~ /a-z/msx) {
    $expanded_set2_138 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_138 =~ /A-Z/msx) {
    $expanded_set2_138 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_137_1 = q{};
    for my $char ( split //msx, $input_138 ) {
    my $pos_138 = index $expanded_set1_138, $char;
    if ( $pos_138 >= 0 && $pos_138 < length $expanded_set2_138 ) {
    $tr_result_137_1 .= substr $expanded_set2_138, $pos_138, 1;
    } else {
    $tr_result_137_1 .= $char;
    }
    }
    if (!($tr_result_137_1 =~ m{\n\z}msx || $tr_result_137_1 eq q{})) {
    $tr_result_137_1 .= "\n";
    }
    $output_137 = $tr_result_137_1;
    $output_137 = $tr_result_137_1;

        my $grep_result_137_2;
    my @grep_lines_137_2 = split /\n/msx, $output_137;
    my @grep_filtered_137_2 = grep { /hello/msx } @grep_lines_137_2;
    $grep_result_137_2 = join "\n", @grep_filtered_137_2;
    if (!($grep_result_137_2 =~ m{\n\z}msx || $grep_result_137_2 eq q{})) {
    $grep_result_137_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_137_2 > 0 ? 0 : 1;
    $output_137 = $grep_result_137_2;
    $output_137 = $grep_result_137_2;
    if ((scalar @grep_filtered_137_2) == 0) {
        $pipeline_success_137 = 0;
    }
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

        my $grep_result_139_2;
    my @grep_lines_139_2 = split /\n/msx, $output_139;
    my @grep_filtered_139_2 = grep { /hello/msx } @grep_lines_139_2;
    $grep_result_139_2 = join "\n", @grep_filtered_139_2;
    if (!($grep_result_139_2 =~ m{\n\z}msx || $grep_result_139_2 eq q{})) {
    $grep_result_139_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_139_2 > 0 ? 0 : 1;
    $output_139 = $grep_result_139_2;
    $output_139 = $grep_result_139_2;
    if ((scalar @grep_filtered_139_2) == 0) {
        $pipeline_success_139 = 0;
    }
    if ($output_139 ne q{} && !defined $output_printed_139) {
        print $output_139;
        if (!($output_139 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_139 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

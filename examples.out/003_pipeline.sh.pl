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

# Original bash: ls | grep "\.txt$" | wc -l
{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do {
    my @ls_files_1 = ();
    if ( -f q{.} ) {
    push @ls_files_1, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_1, $file;
    }
    closedir $dh;
    @ls_files_1 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_1;
    }
    }
    (@ls_files_1 ? join("\n", @ls_files_1) . "\n" : q{});
    };

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /[.]txt$/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
    if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
    $grep_result_0_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }

        use IPC::Open3;
    my @wc_args_0_2 = ('-l');
    my ($wc_in_0_2, $wc_out_0_2, $wc_err_0_2);
    my $wc_pid_0_2 = open3($wc_in_0_2, $wc_out_0_2, $wc_err_0_2, 'wc', @wc_args_0_2);
    print {$wc_in_0_2} $output_0;
    close $wc_in_0_2 or die "Close failed: $OS_ERROR\n";
    my $output_0_2 = do { local $/ = undef; <$wc_out_0_2> };
    if ($output_0_2 eq q{}) { $output_0_2 = "0\n"; }
    close $wc_out_0_2 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_0_2, 0;
    $output_0 = $output_0_2;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_3 = q{};
    my $output_printed_3;
    my $pipeline_success_3 = 1;
        $output_3 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($output_3 eq q{}) {
        $pipeline_success_3 = 0;
    }

        my @sort_lines_3_1 = split /\n/msx, $output_3;
    my @sort_sorted_3_1 = sort @sort_lines_3_1;
    my $output_3_1 = join "\n", @sort_sorted_3_1;
    if ($output_3_1 ne q{} && !($output_3_1 =~ m{\n\z}msx)) {
    $output_3_1 .= "\n";
    }
    $output_3 = $output_3_1;
    $output_3 = $output_3_1;

        my @uniq_lines_3_2 = split /\n/msx, $output_3;
    @uniq_lines_3_2 = grep { $_ ne q{} } @uniq_lines_3_2; # Filter out empty lines
    my %uniq_counts_3_2;
    my @uniq_order_3_2;
    foreach my $line (@uniq_lines_3_2) {
    if (!exists $uniq_counts_3_2{$line}) { push @uniq_order_3_2, $line; }
    $uniq_counts_3_2{$line}++;
    }
    my @uniq_result_3_2;
    foreach my $line (@uniq_order_3_2) {
    push @uniq_result_3_2, sprintf "%7d %s", $uniq_counts_3_2{$line}, $line;
    }
    my $output_3_2 = join "\n", @uniq_result_3_2;
    if ($output_3_2 ne q{} && !($output_3_2 =~ m{\n\z}msx)) {
    $output_3_2 .= "\n";
    }
    $output_3 = $output_3_2;

        my @sort_lines_3_3 = split /\n/msx, $output_3;
    my @sort_sorted_3_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_3_3;
    @sort_sorted_3_3 = reverse @sort_sorted_3_3;
    my $output_3_3 = join "\n", @sort_sorted_3_3;
    if ($output_3_3 ne q{} && !($output_3_3 =~ m{\n\z}msx)) {
    $output_3_3 .= "\n";
    }
    $output_3 = $output_3_3;
    $output_3 = $output_3_3;
    if ($output_3 ne q{} && !defined $output_printed_3) {
        print $output_3;
        if (!($output_3 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_3 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_4 = q{};
    my $output_printed_4;
    my $pipeline_success_4 = 1;
        $output_4 = do {
    use File::Basename;
    my @files_5 = ();
    my $start_5 = q{.};
    my $_find_5;
    $_find_5 = sub {
    my ($dir_5, $depth_5) = @_;
    opendir(my $dh_5, $dir_5) or return;
    my @entries_5 = readdir($dh_5);
    closedir($dh_5);
    for my $entry_5 (@entries_5) {
    next if $entry_5 eq q{.} || $entry_5 eq q{..};
    my $file_5 = "$dir_5/$entry_5";
    if (-d $file_5) {
    $_find_5->($file_5, $depth_5 + 1);
    }
    elsif (-f $file_5) {
    next if !( basename($file_5) =~ m/^.*.sh$/xms );
    push @files_5, $file_5;
    }
    }
    };
    $_find_5->($start_5, 0);
    join "\n", @files_5;
    };

        my @xargs_files_4_1 = split /\n/msx, $output_4;
    my @xargs_matching_files_4_1;
    foreach my $file (@xargs_files_4_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_4_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_4_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_4_1) { push @xargs_matching_files_4_1, $file; }
    }
    }
    my $xargs_result_4_1 = join "\n", @xargs_matching_files_4_1;
    if (!($xargs_result_4_1 =~ m{\n\z}msx)) {
    $xargs_result_4_1 .= "\n";
    }
    $output_4 = $xargs_result_4_1;

        my $set1_6 = "\\\\/";
    my $input_6 = $output_4;
    my $tr_result_4_2 = q{};
    for my $char ( split //msx, $input_6 ) {
    if ( (index $set1_6, $char) == -1 ) {
    $tr_result_4_2 .= $char;
    }
    }
    if (!($tr_result_4_2 =~ m{\n\z}msx || $tr_result_4_2 eq q{})) {
    $tr_result_4_2 .= "\n";
    }
    $output_4 = $tr_result_4_2;
    $output_4 = $tr_result_4_2;
    if ($output_4 ne q{} && !defined $output_printed_4) {
        print $output_4;
        if (!($output_4 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_4 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_7 = q{};
    my $output_printed_7;
    my $pipeline_success_7 = 1;
        $output_7 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($output_7 eq q{}) {
        $pipeline_success_7 = 0;
    }

        my $set1_8 = q{a};
    my $set2_8 = q{b};
    my $input_8 = $output_7;
    # Expand character ranges for tr command
    my $expanded_set1_8 = $set1_8;
    my $expanded_set2_8 = $set2_8;
    # Handle a-z range in set1
    if ($expanded_set1_8 =~ /a-z/msx) {
    $expanded_set1_8 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_8 =~ /A-Z/msx) {
    $expanded_set1_8 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_8 =~ /a-z/msx) {
    $expanded_set2_8 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_8 =~ /A-Z/msx) {
    $expanded_set2_8 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_7_1 = q{};
    for my $char ( split //msx, $input_8 ) {
    my $pos_8 = index $expanded_set1_8, $char;
    if ( $pos_8 >= 0 && $pos_8 < length $expanded_set2_8 ) {
    $tr_result_7_1 .= substr $expanded_set2_8, $pos_8, 1;
    } else {
    $tr_result_7_1 .= $char;
    }
    }
    if (!($tr_result_7_1 =~ m{\n\z}msx || $tr_result_7_1 eq q{})) {
    $tr_result_7_1 .= "\n";
    }
    $output_7 = $tr_result_7_1;
    $output_7 = $tr_result_7_1;

        my $grep_result_7_2;
    my @grep_lines_7_2 = split /\n/msx, $output_7;
    my @grep_filtered_7_2 = grep { /hello/msx } @grep_lines_7_2;
    $grep_result_7_2 = join "\n", @grep_filtered_7_2;
    if (!($grep_result_7_2 =~ m{\n\z}msx || $grep_result_7_2 eq q{})) {
    $grep_result_7_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_7_2 > 0 ? 0 : 1;
    $output_7 = $grep_result_7_2;
    $output_7 = $grep_result_7_2;
    if ((scalar @grep_filtered_7_2) == 0) {
        $pipeline_success_7 = 0;
    }
    if ($output_7 ne q{} && !defined $output_printed_7) {
        print $output_7;
        if (!($output_7 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_7 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
{
    my $output_9 = q{};
    my $output_printed_9;
    my $pipeline_success_9 = 1;
        $output_9 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($output_9 eq q{}) {
        $pipeline_success_9 = 0;
    }

        my @sort_lines_9_1 = split /\n/msx, $output_9;
    my @sort_sorted_9_1 = sort @sort_lines_9_1;
    my $output_9_1 = join "\n", @sort_sorted_9_1;
    if ($output_9_1 ne q{} && !($output_9_1 =~ m{\n\z}msx)) {
    $output_9_1 .= "\n";
    }
    $output_9 = $output_9_1;
    $output_9 = $output_9_1;

        my $grep_result_9_2;
    my @grep_lines_9_2 = split /\n/msx, $output_9;
    my @grep_filtered_9_2 = grep { /hello/msx } @grep_lines_9_2;
    $grep_result_9_2 = join "\n", @grep_filtered_9_2;
    if (!($grep_result_9_2 =~ m{\n\z}msx || $grep_result_9_2 eq q{})) {
    $grep_result_9_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_9_2 > 0 ? 0 : 1;
    $output_9 = $grep_result_9_2;
    $output_9 = $grep_result_9_2;
    if ((scalar @grep_filtered_9_2) == 0) {
        $pipeline_success_9 = 0;
    }
    if ($output_9 ne q{} && !defined $output_printed_9) {
        print $output_9;
        if (!($output_9 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_9 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

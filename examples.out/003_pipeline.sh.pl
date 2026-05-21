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
    my $output_139 = q{};
    my $output_printed_139;
    my $pipeline_success_139 = 1;
        $output_139 = do {
    my @ls_files_140 = ();
    if ( -f q{.} ) {
    push @ls_files_140, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_140, $file;
    }
    closedir $dh;
    @ls_files_140 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_140;
    }
    }
    (@ls_files_140 ? join("\n", @ls_files_140) . "\n" : q{});
    };

        my $grep_result_139_1;
    my @grep_lines_139_1 = split /\n/msx, $output_139;
    my @grep_filtered_139_1 = grep { /[.]txt$/msx } @grep_lines_139_1;
    $grep_result_139_1 = join "\n", @grep_filtered_139_1;
    if (!($grep_result_139_1 =~ m{\n\z}msx || $grep_result_139_1 eq q{})) {
    $grep_result_139_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_139_1 > 0 ? 0 : 1;
    $output_139 = $grep_result_139_1;
    $output_139 = $grep_result_139_1;
    if ((scalar @grep_filtered_139_1) == 0) {
        $pipeline_success_139 = 0;
    }

        use IPC::Open3;
    my @wc_args_139_2 = ('-l');
    my ($wc_in_139_2, $wc_out_139_2, $wc_err_139_2);
    my $wc_pid_139_2 = open3($wc_in_139_2, $wc_out_139_2, $wc_err_139_2, 'wc', @wc_args_139_2);
    print {$wc_in_139_2} $output_139;
    close $wc_in_139_2 or die "Close failed: $OS_ERROR\n";
    my $output_139_2 = do { local $/ = undef; <$wc_out_139_2> };
    if ($output_139_2 eq q{}) { $output_139_2 = "0\n"; }
    close $wc_out_139_2 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_139_2, 0;
    $output_139 = $output_139_2;
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
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_142 = q{};
    my $output_printed_142;
    my $pipeline_success_142 = 1;
        $output_142 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($output_142 eq q{}) {
        $pipeline_success_142 = 0;
    }

        my @sort_lines_142_1 = split /\n/msx, $output_142;
    my @sort_sorted_142_1 = sort @sort_lines_142_1;
    my $output_142_1 = join "\n", @sort_sorted_142_1;
    if ($output_142_1 ne q{} && !($output_142_1 =~ m{\n\z}msx)) {
    $output_142_1 .= "\n";
    }
    $output_142 = $output_142_1;
    $output_142 = $output_142_1;

        my @uniq_lines_142_2 = split /\n/msx, $output_142;
    @uniq_lines_142_2 = grep { $_ ne q{} } @uniq_lines_142_2; # Filter out empty lines
    my %uniq_counts_142_2;
    my @uniq_order_142_2;
    foreach my $line (@uniq_lines_142_2) {
    if (!exists $uniq_counts_142_2{$line}) { push @uniq_order_142_2, $line; }
    $uniq_counts_142_2{$line}++;
    }
    my @uniq_result_142_2;
    foreach my $line (@uniq_order_142_2) {
    push @uniq_result_142_2, sprintf "%7d %s", $uniq_counts_142_2{$line}, $line;
    }
    my $output_142_2 = join "\n", @uniq_result_142_2;
    if ($output_142_2 ne q{} && !($output_142_2 =~ m{\n\z}msx)) {
    $output_142_2 .= "\n";
    }
    $output_142 = $output_142_2;

        my @sort_lines_142_3 = split /\n/msx, $output_142;
    my @sort_sorted_142_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_142_3;
    @sort_sorted_142_3 = reverse @sort_sorted_142_3;
    my $output_142_3 = join "\n", @sort_sorted_142_3;
    if ($output_142_3 ne q{} && !($output_142_3 =~ m{\n\z}msx)) {
    $output_142_3 .= "\n";
    }
    $output_142 = $output_142_3;
    $output_142 = $output_142_3;
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
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_143 = q{};
    my $output_printed_143;
    my $pipeline_success_143 = 1;
        $output_143 = do {
    use File::Basename;
    my @files_144 = ();
    my $start_144 = q{.};
    my $_find_144;
    $_find_144 = sub {
    my ($dir_144, $depth_144) = @_;
    opendir(my $dh_144, $dir_144) or return;
    my @entries_144 = readdir($dh_144);
    closedir($dh_144);
    for my $entry_144 (@entries_144) {
    next if $entry_144 eq q{.} || $entry_144 eq q{..};
    my $file_144 = "$dir_144/$entry_144";
    if (-d $file_144) {
    $_find_144->($file_144, $depth_144 + 1);
    }
    elsif (-f $file_144) {
    next if !( basename($file_144) =~ m/^.*.sh$/xms );
    push @files_144, $file_144;
    }
    }
    };
    $_find_144->($start_144, 0);
    join "\n", @files_144;
    };

        my @xargs_files_143_1 = split /\n/msx, $output_143;
    my @xargs_matching_files_143_1;
    foreach my $file (@xargs_files_143_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_143_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_143_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_143_1) { push @xargs_matching_files_143_1, $file; }
    }
    }
    my $xargs_result_143_1 = join "\n", @xargs_matching_files_143_1;
    if (!($xargs_result_143_1 =~ m{\n\z}msx)) {
    $xargs_result_143_1 .= "\n";
    }
    $output_143 = $xargs_result_143_1;

        my $set1_145 = "\\\\/";
    my $input_145 = $output_143;
    my $tr_result_143_2 = q{};
    for my $char ( split //msx, $input_145 ) {
    if ( (index $set1_145, $char) == -1 ) {
    $tr_result_143_2 .= $char;
    }
    }
    if (!($tr_result_143_2 =~ m{\n\z}msx || $tr_result_143_2 eq q{})) {
    $tr_result_143_2 .= "\n";
    }
    $output_143 = $tr_result_143_2;
    $output_143 = $tr_result_143_2;
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
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_146 = q{};
    my $output_printed_146;
    my $pipeline_success_146 = 1;
        $output_146 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($output_146 eq q{}) {
        $pipeline_success_146 = 0;
    }

        my $set1_147 = q{a};
    my $set2_147 = q{b};
    my $input_147 = $output_146;
    # Expand character ranges for tr command
    my $expanded_set1_147 = $set1_147;
    my $expanded_set2_147 = $set2_147;
    # Handle a-z range in set1
    if ($expanded_set1_147 =~ /a-z/msx) {
    $expanded_set1_147 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_147 =~ /A-Z/msx) {
    $expanded_set1_147 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_147 =~ /a-z/msx) {
    $expanded_set2_147 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_147 =~ /A-Z/msx) {
    $expanded_set2_147 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_146_1 = q{};
    for my $char ( split //msx, $input_147 ) {
    my $pos_147 = index $expanded_set1_147, $char;
    if ( $pos_147 >= 0 && $pos_147 < length $expanded_set2_147 ) {
    $tr_result_146_1 .= substr $expanded_set2_147, $pos_147, 1;
    } else {
    $tr_result_146_1 .= $char;
    }
    }
    if (!($tr_result_146_1 =~ m{\n\z}msx || $tr_result_146_1 eq q{})) {
    $tr_result_146_1 .= "\n";
    }
    $output_146 = $tr_result_146_1;
    $output_146 = $tr_result_146_1;

        my $grep_result_146_2;
    my @grep_lines_146_2 = split /\n/msx, $output_146;
    my @grep_filtered_146_2 = grep { /hello/msx } @grep_lines_146_2;
    $grep_result_146_2 = join "\n", @grep_filtered_146_2;
    if (!($grep_result_146_2 =~ m{\n\z}msx || $grep_result_146_2 eq q{})) {
    $grep_result_146_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_146_2 > 0 ? 0 : 1;
    $output_146 = $grep_result_146_2;
    $output_146 = $grep_result_146_2;
    if ((scalar @grep_filtered_146_2) == 0) {
        $pipeline_success_146 = 0;
    }
    if ($output_146 ne q{} && !defined $output_printed_146) {
        print $output_146;
        if (!($output_146 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_146 ) { $main_exit_code = 1; }
    }
print "\n";
$CHILD_ERROR = 0;
{
    my $output_148 = q{};
    my $output_printed_148;
    my $pipeline_success_148 = 1;
        $output_148 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'file.txt' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($output_148 eq q{}) {
        $pipeline_success_148 = 0;
    }

        my @sort_lines_148_1 = split /\n/msx, $output_148;
    my @sort_sorted_148_1 = sort @sort_lines_148_1;
    my $output_148_1 = join "\n", @sort_sorted_148_1;
    if ($output_148_1 ne q{} && !($output_148_1 =~ m{\n\z}msx)) {
    $output_148_1 .= "\n";
    }
    $output_148 = $output_148_1;
    $output_148 = $output_148_1;

        my $grep_result_148_2;
    my @grep_lines_148_2 = split /\n/msx, $output_148;
    my @grep_filtered_148_2 = grep { /hello/msx } @grep_lines_148_2;
    $grep_result_148_2 = join "\n", @grep_filtered_148_2;
    if (!($grep_result_148_2 =~ m{\n\z}msx || $grep_result_148_2 eq q{})) {
    $grep_result_148_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_148_2 > 0 ? 0 : 1;
    $output_148 = $grep_result_148_2;
    $output_148 = $grep_result_148_2;
    if ((scalar @grep_filtered_148_2) == 0) {
        $pipeline_success_148 = 0;
    }
    if ($output_148 ne q{} && !defined $output_printed_148) {
        print $output_148;
        if (!($output_148 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_148 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

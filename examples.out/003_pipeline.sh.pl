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
    @ls_files_140 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_140;
    }
    }
    (@ls_files_140 ? join("\n", @ls_files_140) . "\n" : q{});
    };
    ;

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
    close $wc_in_139_2 or die "Close failed: $!\n";
    my $output_139_2 = do { local $/ = undef; <$wc_out_139_2> };
    if ($output_139_2 eq q{}) { $output_139_2 = "0\n"; }
    close $wc_out_139_2 or die "Close failed: $!\n";
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
        $output_142 = do { if (open my $fh, '<', 'file.txt') { local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or print STDERR 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } else { print STDERR 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; $CHILD_ERROR = 1; q{}; } };

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
        $output_143 = do { my $command = q{find . -name '*.sh'}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };

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

        my $set1_144 = "\\\\/";
    my $input_144 = $output_143;
    my $tr_result_143_2 = q{};
    for my $char ( split //msx, $input_144 ) {
    if ( (index $set1_144, $char) == -1 ) {
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
    my $output_145 = q{};
    my $output_printed_145;
    my $pipeline_success_145 = 1;
        $output_145 = do { if (open my $fh, '<', 'file.txt') { local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or print STDERR 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } else { print STDERR 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; $CHILD_ERROR = 1; q{}; } };

        my $set1_146 = q{a};
    my $set2_146 = q{b};
    my $input_146 = $output_145;
    # Expand character ranges for tr command
    my $expanded_set1_146 = $set1_146;
    my $expanded_set2_146 = $set2_146;
    # Handle a-z range in set1
    if ($expanded_set1_146 =~ /a-z/msx) {
    $expanded_set1_146 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_146 =~ /A-Z/msx) {
    $expanded_set1_146 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_146 =~ /a-z/msx) {
    $expanded_set2_146 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_146 =~ /A-Z/msx) {
    $expanded_set2_146 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_145_1 = q{};
    for my $char ( split //msx, $input_146 ) {
    my $pos_146 = index $expanded_set1_146, $char;
    if ( $pos_146 >= 0 && $pos_146 < length $expanded_set2_146 ) {
    $tr_result_145_1 .= substr $expanded_set2_146, $pos_146, 1;
    } else {
    $tr_result_145_1 .= $char;
    }
    }
    if (!($tr_result_145_1 =~ m{\n\z}msx || $tr_result_145_1 eq q{})) {
    $tr_result_145_1 .= "\n";
    }
    $output_145 = $tr_result_145_1;
    $output_145 = $tr_result_145_1;

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
print "\n";
$CHILD_ERROR = 0;
{
    my $output_147 = q{};
    my $output_printed_147;
    my $pipeline_success_147 = 1;
        $output_147 = do { if (open my $fh, '<', 'file.txt') { local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or print STDERR 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; } else { print STDERR 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; $CHILD_ERROR = 1; q{}; } };

        my @sort_lines_147_1 = split /\n/msx, $output_147;
    my @sort_sorted_147_1 = sort @sort_lines_147_1;
    my $output_147_1 = join "\n", @sort_sorted_147_1;
    if ($output_147_1 ne q{} && !($output_147_1 =~ m{\n\z}msx)) {
    $output_147_1 .= "\n";
    }
    $output_147 = $output_147_1;
    $output_147 = $output_147_1;

        my $grep_result_147_2;
    my @grep_lines_147_2 = split /\n/msx, $output_147;
    my @grep_filtered_147_2 = grep { /hello/msx } @grep_lines_147_2;
    $grep_result_147_2 = join "\n", @grep_filtered_147_2;
    if (!($grep_result_147_2 =~ m{\n\z}msx || $grep_result_147_2 eq q{})) {
    $grep_result_147_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_147_2 > 0 ? 0 : 1;
    $output_147 = $grep_result_147_2;
    $output_147 = $grep_result_147_2;
    if ((scalar @grep_filtered_147_2) == 0) {
        $pipeline_success_147 = 0;
    }
    if ($output_147 ne q{} && !defined $output_printed_147) {
        print $output_147;
        if (!($output_147 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_147 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

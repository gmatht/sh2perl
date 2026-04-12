#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

# Original bash: ls | grep "\.txt$" | wc -l
{
    my $output_146;
    my $output_printed_146;
    my $pipeline_success_146 = 1;
        $output_146 = do {
    my @ls_files_147 = ();
    if ( -f q{.} ) {
    push @ls_files_147, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_147, $file;
    }
    closedir $dh;
    @ls_files_147 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_147;
    }
    }
    (@ls_files_147 ? join("\n", @ls_files_147) . "\n" : q{});
    };

        my $grep_result_146_1;
    my @grep_lines_146_1 = split /\n/msx, $output_146;
    my @grep_filtered_146_1 = grep { /[.]txt$/msx } @grep_lines_146_1;
    $grep_result_146_1 = join "\n", @grep_filtered_146_1;
    if (!($grep_result_146_1 =~ m{\n\z}msx || $grep_result_146_1 eq q{})) {
    $grep_result_146_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_146_1 > 0 ? 0 : 1;
    $output_146 = $grep_result_146_1;
    $output_146 = $grep_result_146_1;
    if ((scalar @grep_filtered_146_1) == 0) {
        $pipeline_success_146 = 0;
    }

        use IPC::Open3;
    my @wc_args_146_2 = ("-l");
    my ($wc_in_146_2, $wc_out_146_2, $wc_err_146_2);
    my $wc_pid_146_2 = open3($wc_in_146_2, $wc_out_146_2, $wc_err_146_2, 'wc', @wc_args_146_2);
    print {$wc_in_146_2} $output_146;
    close $wc_in_146_2 or die "Close failed: $!\n";
    my $output_146_2 = do { local $/ = undef; <$wc_out_146_2> };
    close $wc_out_146_2 or die "Close failed: $!\n";
    waitpid $wc_pid_146_2, 0;
    $output_146 = $output_146_2;
    if ($output_146 ne q{} && !defined $output_printed_146) {
        print $output_146;
        if (!($output_146 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_146 ) { $main_exit_code = 1; }
    }
print "\n";
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_149;
    my $output_printed_149;
    my $pipeline_success_149 = 1;
        do { my $cat_cmd = 'cat file.txt'; $output_149 = qx{$cat_cmd}; };
    if ($output_149 eq q{}) {
        $pipeline_success_149 = 0;
    }

        my @sort_lines_149_1 = split /\n/msx, $output_149;
    my @sort_sorted_149_1 = sort @sort_lines_149_1;
    my $output_149_1 = join "\n", @sort_sorted_149_1;
    if ($output_149_1 ne q{} && !($output_149_1 =~ m{\n\z}msx)) {
    $output_149_1 .= "\n";
    }
    $output_149 = $output_149_1;
    $output_149 = $output_149_1;

        my @uniq_lines_149_2 = split /\n/msx, $output_149;
    @uniq_lines_149_2 = grep { $_ ne q{} } @uniq_lines_149_2; # Filter out empty lines
    my %uniq_counts_149_2;
    foreach my $line (@uniq_lines_149_2) {
    $uniq_counts_149_2{$line}++;
    }
    my @uniq_result_149_2;
    foreach my $line (keys %uniq_counts_149_2) {
    push @uniq_result_149_2, sprintf "%7d %s", $uniq_counts_149_2{$line}, $line;
    }
    my $output_149_2 = join "\n", @uniq_result_149_2;
    if ($output_149_2 ne q{} && !($output_149_2 =~ m{\n\z}msx)) {
    $output_149_2 .= "\n";
    }
    $output_149 = $output_149_2;

        my @sort_lines_149_3 = split /\n/msx, $output_149;
    sub sort_numeric_149_3 {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    if ( scalar @a_fields > 0 && $a_fields[0] =~ /^\d+$/msx ) { $a_num = $a_fields[0]; }
    if ( scalar @b_fields > 0 && $b_fields[0] =~ /^\d+$/msx ) { $b_num = $b_fields[0]; }
    return $a_num <=> $b_num || $a cmp $b;
    }
    my @sort_sorted_149_3 = sort sort_numeric_149_3 @sort_lines_149_3;
    @sort_sorted_149_3 = reverse @sort_sorted_149_3;
    my $output_149_3 = join "\n", @sort_sorted_149_3;
    if ($output_149_3 ne q{} && !($output_149_3 =~ m{\n\z}msx)) {
    $output_149_3 .= "\n";
    }
    $output_149 = $output_149_3;
    $output_149 = $output_149_3;
    if ($output_149 ne q{} && !defined $output_printed_149) {
        print $output_149;
        if (!($output_149 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_149 ) { $main_exit_code = 1; }
    }
print "\n";
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_150;
    my $output_printed_150;
    my $pipeline_success_150 = 1;
        $output_150 = do {
    use File::Find;
    use File::Basename;
    my @files_151 = ();
    my $start_151 = q{.};
    sub find_files_151 {
    my $file_151 = $File::Find::name;
    push @files_151, $file_151;
    return;
    }
    find( \&find_files_151, $start_151 );
    join "\n", @files_151;
    };

        my @xargs_files_150_1 = split /\n/msx, $output_150;
    my @xargs_matching_files_150_1;
    foreach my $file (@xargs_files_150_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_150_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_150_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_150_1) { push @xargs_matching_files_150_1, $file; }
    }
    }
    my $xargs_result_150_1 = join "\n", @xargs_matching_files_150_1;
    if (!($xargs_result_150_1 =~ m{\n\z}msx)) {
    $xargs_result_150_1 .= "\n";
    }
    $output_150 = $xargs_result_150_1;

        my $set1_152 = "\\\\/";
    my $input_152 = $output_150;
    my $tr_result_150_2 = q{};
    for my $char ( split //msx, $input_152 ) {
    if ( (index $set1_152, $char) == -1 ) {
    $tr_result_150_2 .= $char;
    }
    }
    if (!($tr_result_150_2 =~ m{\n\z}msx || $tr_result_150_2 eq q{})) {
    $tr_result_150_2 .= "\n";
    }
    $output_150 = $tr_result_150_2;
    if ($output_150 ne q{} && !defined $output_printed_150) {
        print $output_150;
        if (!($output_150 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_150 ) { $main_exit_code = 1; }
    }
print "\n";
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_153;
    my $output_printed_153;
    my $pipeline_success_153 = 1;
        do { my $cat_cmd = 'cat file.txt'; $output_153 = qx{$cat_cmd}; };
    if ($output_153 eq q{}) {
        $pipeline_success_153 = 0;
    }

        my $set1_154 = q{a};
    my $set2_154 = q{b};
    my $input_154 = $output_153;
    # Expand character ranges for tr command
    my $expanded_set1_154 = $set1_154;
    my $expanded_set2_154 = $set2_154;
    # Handle a-z range in set1
    if ($expanded_set1_154 =~ /a-z/msx) {
    $expanded_set1_154 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_154 =~ /A-Z/msx) {
    $expanded_set1_154 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_154 =~ /a-z/msx) {
    $expanded_set2_154 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_154 =~ /A-Z/msx) {
    $expanded_set2_154 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_153_1 = q{};
    for my $char ( split //msx, $input_154 ) {
    my $pos_154 = index $expanded_set1_154, $char;
    if ( $pos_154 >= 0 && $pos_154 < length $expanded_set2_154 ) {
    $tr_result_153_1 .= substr $expanded_set2_154, $pos_154, 1;
    } else {
    $tr_result_153_1 .= $char;
    }
    }
    if (!($tr_result_153_1 =~ m{\n\z}msx || $tr_result_153_1 eq q{})) {
    $tr_result_153_1 .= "\n";
    }
    $output_153 = $tr_result_153_1;

        my $grep_result_153_2;
    my @grep_lines_153_2 = split /\n/msx, $output_153;
    my @grep_filtered_153_2 = grep { /hello/msx } @grep_lines_153_2;
    $grep_result_153_2 = join "\n", @grep_filtered_153_2;
    if (!($grep_result_153_2 =~ m{\n\z}msx || $grep_result_153_2 eq q{})) {
    $grep_result_153_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_153_2 > 0 ? 0 : 1;
    $output_153 = $grep_result_153_2;
    $output_153 = $grep_result_153_2;
    if ((scalar @grep_filtered_153_2) == 0) {
        $pipeline_success_153 = 0;
    }
    if ($output_153 ne q{} && !defined $output_printed_153) {
        print $output_153;
        if (!($output_153 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_153 ) { $main_exit_code = 1; }
    }
print "\n";
{
    my $output_155;
    my $output_printed_155;
    my $pipeline_success_155 = 1;
        do { my $cat_cmd = 'cat file.txt'; $output_155 = qx{$cat_cmd}; };
    if ($output_155 eq q{}) {
        $pipeline_success_155 = 0;
    }

        my @sort_lines_155_1 = split /\n/msx, $output_155;
    my @sort_sorted_155_1 = sort @sort_lines_155_1;
    my $output_155_1 = join "\n", @sort_sorted_155_1;
    if ($output_155_1 ne q{} && !($output_155_1 =~ m{\n\z}msx)) {
    $output_155_1 .= "\n";
    }
    $output_155 = $output_155_1;
    $output_155 = $output_155_1;

        my $grep_result_155_2;
    my @grep_lines_155_2 = split /\n/msx, $output_155;
    my @grep_filtered_155_2 = grep { /hello/msx } @grep_lines_155_2;
    $grep_result_155_2 = join "\n", @grep_filtered_155_2;
    if (!($grep_result_155_2 =~ m{\n\z}msx || $grep_result_155_2 eq q{})) {
    $grep_result_155_2 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_155_2 > 0 ? 0 : 1;
    $output_155 = $grep_result_155_2;
    $output_155 = $grep_result_155_2;
    if ((scalar @grep_filtered_155_2) == 0) {
        $pipeline_success_155 = 0;
    }
    if ($output_155 ne q{} && !defined $output_printed_155) {
        print $output_155;
        if (!($output_155 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_155 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

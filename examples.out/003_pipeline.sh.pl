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
    my $output_138 = q{};
    my $output_printed_138;
    my $pipeline_success_138 = 1;
        $output_138 = do {
    my @ls_files_139 = ();
    if ( -f q{.} ) {
    push @ls_files_139, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_139, $file;
    }
    closedir $dh;
    @ls_files_139 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_139;
    }
    }
    (@ls_files_139 ? join("\n", @ls_files_139) . "\n" : q{});
    };

        my $grep_result_138_1;
    my @grep_lines_138_1 = split /\n/msx, $output_138;
    my @grep_filtered_138_1 = grep { /[.]txt$/msx } @grep_lines_138_1;
    $grep_result_138_1 = join "\n", @grep_filtered_138_1;
    if (!($grep_result_138_1 =~ m{\n\z}msx || $grep_result_138_1 eq q{})) {
    $grep_result_138_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_138_1 > 0 ? 0 : 1;
    $output_138 = $grep_result_138_1;
    $output_138 = $grep_result_138_1;
    if ((scalar @grep_filtered_138_1) == 0) {
        $pipeline_success_138 = 0;
    }

        use IPC::Open3;
    my @wc_args_138_2 = ('-l');
    my ($wc_in_138_2, $wc_out_138_2, $wc_err_138_2);
    my $wc_pid_138_2 = open3($wc_in_138_2, $wc_out_138_2, $wc_err_138_2, 'wc', @wc_args_138_2);
    print {$wc_in_138_2} $output_138;
    close $wc_in_138_2 or die "Close failed: $!\n";
    my $output_138_2 = do { local $/ = undef; <$wc_out_138_2> };
    if ($output_138_2 eq q{}) { $output_138_2 = "0\n"; }
    close $wc_out_138_2 or die "Close failed: $!\n";
    waitpid $wc_pid_138_2, 0;
    $output_138 = $output_138_2;
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
# Original bash: cat file.txt | sort | uniq -c | sort -nr
{
    my $output_141 = q{};
    my $output_printed_141;
    my $pipeline_success_141 = 1;
        $output_141 = do { open my $fh, '<', 'file.txt' or die 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; };
    if ($output_141 eq q{}) {
        $pipeline_success_141 = 0;
    }

        my @sort_lines_141_1 = split /\n/msx, $output_141;
    my @sort_sorted_141_1 = sort @sort_lines_141_1;
    my $output_141_1 = join "\n", @sort_sorted_141_1;
    if ($output_141_1 ne q{} && !($output_141_1 =~ m{\n\z}msx)) {
    $output_141_1 .= "\n";
    }
    $output_141 = $output_141_1;
    $output_141 = $output_141_1;

        my @uniq_lines_141_2 = split /\n/msx, $output_141;
    @uniq_lines_141_2 = grep { $_ ne q{} } @uniq_lines_141_2; # Filter out empty lines
    my %uniq_counts_141_2;
    my @uniq_order_141_2;
    foreach my $line (@uniq_lines_141_2) {
    if (!exists $uniq_counts_141_2{$line}) { push @uniq_order_141_2, $line; }
    $uniq_counts_141_2{$line}++;
    }
    my @uniq_result_141_2;
    foreach my $line (@uniq_order_141_2) {
    push @uniq_result_141_2, sprintf "%7d %s", $uniq_counts_141_2{$line}, $line;
    }
    my $output_141_2 = join "\n", @uniq_result_141_2;
    if ($output_141_2 ne q{} && !($output_141_2 =~ m{\n\z}msx)) {
    $output_141_2 .= "\n";
    }
    $output_141 = $output_141_2;

        my @sort_lines_141_3 = split /\n/msx, $output_141;
    my @sort_sorted_141_3 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_141_3;
    @sort_sorted_141_3 = reverse @sort_sorted_141_3;
    my $output_141_3 = join "\n", @sort_sorted_141_3;
    if ($output_141_3 ne q{} && !($output_141_3 =~ m{\n\z}msx)) {
    $output_141_3 .= "\n";
    }
    $output_141 = $output_141_3;
    $output_141 = $output_141_3;
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
# Original bash: find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"
{
    my $output_142 = q{};
    my $output_printed_142;
    my $pipeline_success_142 = 1;
        $output_142 = do {
    use File::Find;
    use File::Basename;
    my @files_143 = ();
    my $start_143 = q{.};
    find( sub {
    my $file_143 = $File::Find::name;
    if ( !( basename($file_143) =~ m/^.*.sh$/xms ) ) {
    return;
    }
    push @files_143, $file_143;
    },
    $start_143 );
    join "\n", @files_143;
    };

        my @xargs_files_142_1 = split /\n/msx, $output_142;
    my @xargs_matching_files_142_1;
    foreach my $file (@xargs_files_142_1) {
    next if !($file && -f $file);
    if (open my $fh, '<', $file) {
    my $xargs_found_142_1 = 0;
    while (my $line = <$fh>) {
    if ($line =~ /function/msx) {
    $xargs_found_142_1 = 1;
    last;
    }
    }
    close $fh or carp "Close failed: $OS_ERROR";
    if ($xargs_found_142_1) { push @xargs_matching_files_142_1, $file; }
    }
    }
    my $xargs_result_142_1 = join "\n", @xargs_matching_files_142_1;
    if (!($xargs_result_142_1 =~ m{\n\z}msx)) {
    $xargs_result_142_1 .= "\n";
    }
    $output_142 = $xargs_result_142_1;

        my $set1_144 = "\\\\/";
    my $input_144 = $output_142;
    my $tr_result_142_2 = q{};
    for my $char ( split //msx, $input_144 ) {
    if ( (index $set1_144, $char) == -1 ) {
    $tr_result_142_2 .= $char;
    }
    }
    if (!($tr_result_142_2 =~ m{\n\z}msx || $tr_result_142_2 eq q{})) {
    $tr_result_142_2 .= "\n";
    }
    $output_142 = $tr_result_142_2;
    $output_142 = $tr_result_142_2;
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
# Original bash: cat file.txt | tr 'a' 'b' | grep 'hello'
{
    my $output_145 = q{};
    my $output_printed_145;
    my $pipeline_success_145 = 1;
        $output_145 = do { open my $fh, '<', 'file.txt' or die 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; };
    if ($output_145 eq q{}) {
        $pipeline_success_145 = 0;
    }

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
        $output_147 = do { open my $fh, '<', 'file.txt' or die 'cat: ' . 'file.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; };
    if ($output_147 eq q{}) {
        $pipeline_success_147 = 0;
    }

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

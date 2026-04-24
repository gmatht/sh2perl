use Carp;
#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/035_pipeline_basic.pl" }


print "=== Example 035: Basic pipeline ===\n";

open(my $fh, '>', 'test_pipeline.txt') or die "Cannot create test file: $!\n";
print $fh "apple\n";
print $fh "banana\n";
print $fh "cherry\n";
print $fh "date\n";
print $fh "elderberry\n";
print $fh "fig\n";
print $fh "grape\n";
close($fh);

print "Using backticks to call pipeline (cat | grep | sort):\n";
my $pipeline_output = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /a/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
        if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
            $grep_result_0_1 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    my @sort_lines_0_2 = split /\n/msx, $output_0;
    my @sort_sorted_0_2 = sort @sort_lines_0_2;
    $output_0 = join "\n", @sort_sorted_0_2;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0;
} }
;
print $pipeline_output;

print "\nPipeline with multiple commands (cat | grep | wc):\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);use IPC::Open3;{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    if ($output_0 eq q{}) {
        $pipeline_success_0 = 0;
    }

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /a/msx } @grep_lines_0_1;
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
    close $wc_in_0_2 or die "Close failed: $!\n";
    my $output_0_2 = do { local $/ = undef; <$wc_out_0_2> };
    if ($output_0_2 eq q{}) { $output_0_2 = "0\n"; }
    close $wc_out_0_2 or die "Close failed: $!\n";
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

print "\nPipeline with head and tail:\n";
my $pipeline_head_tail = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_0;
    my $pos             = 0;

    while ( $pos < length $input && $head_line_count < $num_lines ) {
        my $line_end = index $input, "\n", $pos;
        if ( $line_end == -1 ) {
            $line_end = length $input;
        }
        my $head_line = substr $input, $pos, $line_end - $pos;
        $result .= $head_line . "\n";
        $pos = $line_end + 1;
        ++$head_line_count;
    }
    $output_0 = $result;

    my @lines = split /\n/msx, $output_0;
    my $num_lines = 3;
    if ($num_lines > scalar @lines) {
    $num_lines = scalar @lines;
    }
    my $start_index = scalar @lines - $num_lines;
    if ($start_index < 0) { $start_index = 0; }
    my @result = @lines[$start_index..$#lines];
    $output_0 = join "\n", @result;

    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0;
} }
;
print $pipeline_head_tail;

print "\nPipeline with sed and awk:\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    if ($output_0 eq q{}) {
        $pipeline_success_0 = 0;
    }

        my @sed_lines_0 = split /\n/msx, $output_0;
    my @sed_result_0;
    foreach my $line (@sed_lines_0) {
    chomp $line;
    $line =~ s/a/A/gmsx;
    push @sed_result_0, $line;
    }
    $output_0 = join "\n", @sed_result_0;

        my @lines = split /\n/msx, $output_0;
    my @result;
    foreach my $line (@lines) {
    chomp $line;
    if ($line =~ /^\\s*$/msx) { next; }
    my @fields = split /\s+/msx, $line;
    if (@fields > 0) {
    push @result, $line;
    }
    }
    $output_0 = join "\n", @result;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\nPipeline with cut and paste:\n";
my $pipeline_cut_paste = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= "1,2,3\n4,5,6\n7,8,9";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines_1 = split /\n/msx, $output_0;
    my @result_1;
    foreach my $line (@lines_1) {
    chomp $line;
    my @fields = split /\t/msx, $line;
    if (@fields > 0) {
    push @result_1, $fields[0];
    }
    }
    $output_0 = join "\n", @result_1;

    $output_0 = do {
        my @paste_file1_lines_fh_1;
        my @paste_file2_lines_fh_1;
        if (open my $fh1, '<', q{-}) {
            while (my $line = <$fh1>) {
                chomp $line;
                push @paste_file1_lines_fh_1, $line;
            }
            close $fh1 or croak "Close failed: $!";
        }
        if (open my $fh2, '<', q{-}) {
            while (my $line = <$fh2>) {
                chomp $line;
                push @paste_file2_lines_fh_1, $line;
            }
            close $fh2 or croak "Close failed: $!";
        }
        my $max_lines = scalar @paste_file1_lines_fh_1 > scalar @paste_file2_lines_fh_1 ? scalar @paste_file1_lines_fh_1 : scalar @paste_file2_lines_fh_1;
        my $paste_output = q{};
        for my $i (0..$max_lines-1) {
            my $line1 = $i < scalar @paste_file1_lines_fh_1 ? $paste_file1_lines_fh_1[$i] : q{};
            my $line2 = $i < scalar @paste_file2_lines_fh_1 ? $paste_file2_lines_fh_1[$i] : q{};
            $paste_output .= "$line1\t$line2\n";
        }
        $paste_output
        };
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0;
} }
;
print $pipeline_cut_paste;

print "\nPipeline with tr and sort:\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    if ($output_0 eq q{}) {
        $pipeline_success_0 = 0;
    }

        my $set1_1 = 'a-z';
    my $set2_1 = 'A-Z';
    my $input_1 = $output_0;
    # Expand character ranges for tr command
    my $expanded_set1_1 = $set1_1;
    my $expanded_set2_1 = $set2_1;
    # Handle a-z range in set1
    if ($expanded_set1_1 =~ /a-z/msx) {
    $expanded_set1_1 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_1 =~ /A-Z/msx) {
    $expanded_set1_1 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_1 =~ /a-z/msx) {
    $expanded_set2_1 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_1 =~ /A-Z/msx) {
    $expanded_set2_1 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_0_1 = q{};
    for my $char ( split //msx, $input_1 ) {
    my $pos_1 = index $expanded_set1_1, $char;
    if ( $pos_1 >= 0 && $pos_1 < length $expanded_set2_1 ) {
    $tr_result_0_1 .= substr $expanded_set2_1, $pos_1, 1;
    } else {
    $tr_result_0_1 .= $char;
    }
    }
    if (!($tr_result_0_1 =~ m{\n\z}msx || $tr_result_0_1 eq q{})) {
    $tr_result_0_1 .= "\n";
    }
    $output_0 = $tr_result_0_1;

        my @sort_lines_0_2 = split /\n/msx, $output_0;
    my @sort_sorted_0_2 = sort @sort_lines_0_2;
    my $output_0_2 = join "\n", @sort_sorted_0_2;
    if ($output_0_2 ne q{} && !($output_0_2 =~ m{\n\z}msx)) {
    $output_0_2 .= "\n";
    }
    $output_0 = $output_0_2;
    $output_0 = $output_0_2;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\nPipeline with uniq and wc:\n";
my $pipeline_uniq_wc = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my @sort_lines_0_1 = split /\n/msx, $output_0;
    my @sort_sorted_0_1 = sort @sort_lines_0_1;
    $output_0 = join "\n", @sort_sorted_0_1;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    my @uniq_lines_0_2 = split /\n/msx, $output_0;
    @uniq_lines_0_2 = grep { $_ ne q{} } @uniq_lines_0_2; 
    my %uniq_seen_0_2;
    my @uniq_result_0_2;
    foreach my $line (@uniq_lines_0_2) {
    if (!$uniq_seen_0_2{$line}++) { push @uniq_result_0_2, $line; }
    }
    $output_0 = join "\n", @uniq_result_0_2;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    use IPC::Open3;
    my @wc_args_0_3 = ('-l');
    my ($wc_in_0_3, $wc_out_0_3, $wc_err_0_3);
    my $wc_pid_0_3 = open3($wc_in_0_3, $wc_out_0_3, $wc_err_0_3, 'wc', @wc_args_0_3);
    print {$wc_in_0_3} $output_0;
    close $wc_in_0_3 or die "Close failed: $!\n";
    $output_0 = do { local $/ = undef; <$wc_out_0_3> };
    if ($output_0 eq q{}) { $output_0 = "0\n"; }
    close $wc_out_0_3 or die "Close failed: $!\n";
    waitpid $wc_pid_0_3, 0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0;
} }
;
print "Unique lines: $pipeline_uniq_wc";

print "\nPipeline with grep and head:\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    if ($output_0 eq q{}) {
        $pipeline_success_0 = 0;
    }

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /e/msx } @grep_lines_0_1;
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

        my $num_lines       = 2;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_0;
    my $pos             = 0;
    while ( $pos < length $input && $head_line_count < $num_lines ) {
    my $line_end = index $input, "\n", $pos;
    if ( $line_end == -1 ) {
    $line_end = length $input;
    }
    my $head_line = substr $input, $pos, $line_end - $pos;
    $result .= $head_line . "\n";
    $pos = $line_end + 1;
    ++$head_line_count;
    }
    $output_0 = $result;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\nPipeline with tail and grep:\n";
my $pipeline_tail_grep = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my @lines = split /\n/msx, $output_0;
    my $num_lines = 5;
    if ($num_lines > scalar @lines) {
    $num_lines = scalar @lines;
    }
    my $start_index = scalar @lines - $num_lines;
    if ($start_index < 0) { $start_index = 0; }
    my @result = @lines[$start_index..$#lines];
    $output_0 = join "\n", @result;

    my $grep_result_0_2;
    my @grep_lines_0_2 = split /\n/msx, $output_0;
    my @grep_filtered_0_2 = grep { /a/msx } @grep_lines_0_2;
    $grep_result_0_2 = join "\n", @grep_filtered_0_2;
        if (!($grep_result_0_2 =~ m{\n\z}msx || $grep_result_0_2 eq q{})) {
            $grep_result_0_2 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_0_2 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_2;
    if ((scalar @grep_filtered_0_2) == 0) {
        $pipeline_success_0 = 0;
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0;
} }
;
print $pipeline_tail_grep;

print "\nPipeline with multiple filters:\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    if ($output_0 eq q{}) {
        $pipeline_success_0 = 0;
    }

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /a/msx } @grep_lines_0_1;
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

        my @sort_lines_0_2 = split /\n/msx, $output_0;
    my @sort_sorted_0_2 = sort @sort_lines_0_2;
    my $output_0_2 = join "\n", @sort_sorted_0_2;
    if ($output_0_2 ne q{} && !($output_0_2 =~ m{\n\z}msx)) {
    $output_0_2 .= "\n";
    }
    $output_0 = $output_0_2;
    $output_0 = $output_0_2;

        my $num_lines       = 3;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_0;
    my $pos             = 0;
    while ( $pos < length $input && $head_line_count < $num_lines ) {
    my $line_end = index $input, "\n", $pos;
    if ( $line_end == -1 ) {
    $line_end = length $input;
    }
    my $head_line = substr $input, $pos, $line_end - $pos;
    $result .= $head_line . "\n";
    $pos = $line_end + 1;
    ++$head_line_count;
    }
    $output_0 = $result;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

print "\nPipeline with error handling:\n";
my $pipeline_error = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /x/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
        if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
            $grep_result_0_1 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0;
} }
;
print "Lines with 'x': $pipeline_error";

print "\nPipeline with tee:\n";
use Carp;use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    if ($output_0 eq q{}) {
        $pipeline_success_0 = 0;
    }

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /a/msx } @grep_lines_0_1;
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

        use Carp qw(carp croak);
    if ( open my $fh, '>', 'pipeline_output.txt' ) {
    print {$fh} $output_0;
    close $fh or Carp::croak "Close failed: $!";
    }
    else {
    carp "tee: Cannot open 'pipeline_output.txt': $!";
    }
    $output_0 = $output_0;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

if (-f "pipeline_output.txt") {
    print "Pipeline output file created\n";
    my $output_content = do { open my $fh, '<', 'pipeline_output.txt' or die 'cat: ' . 'pipeline_output.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "Output content: $output_content";
}

unlink('test_pipeline.txt') if -f 'test_pipeline.txt';
unlink('pipeline_output.txt') if -f 'pipeline_output.txt';

print "=== Example 035 completed successfully ===\n";

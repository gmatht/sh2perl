use Carp;
#!/usr/bin/perl
BEGIN { $0 = "examples.impurl/037_complex_pipeline.pl" }


print "=== Example 037: Complex pipeline ===\n";

open(my $fh, '>', 'test_data.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,Engineer,95.5\n";
print $fh "Bob,30,Manager,87.2\n";
print $fh "Charlie,35,Developer,92.8\n";
print $fh "Diana,28,Designer,88.9\n";
print $fh "Eve,32,Analyst,91.3\n";
print $fh "Frank,29,Engineer,89.7\n";
print $fh "Grace,31,Manager,93.1\n";
print $fh "Henry,27,Developer,86.4\n";
close($fh);

print "Complex pipeline 1: Data processing and filtering\n";
print "cat | grep | cut | sort | head\n";
my $pipeline1 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /Engineer/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
        if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
            $grep_result_0_1 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    my @lines_1 = split /\n/msx, $output_0;
    my @result_1;
    foreach my $line (@lines_1) {
    chomp $line;
    my @fields = split /,/msx, $line;
    my @sel = ();
    if (@fields > 0) { push @sel, $fields[0]; }
    if (@fields > 3) { push @sel, $fields[3]; }
    push @result_1, join(q{,}, @sel);
    }
    $output_0 = join "\n", @result_1;

    my @sort_lines_0_3 = split /\n/msx, $output_0;
    sub sort_numeric_0_3 {
        my @a_fields = split /,/msx, $a;
        my @b_fields = split /,/msx, $b;
        my $a_num = 0;
        my $b_num = 0;
        my $a_key = ( scalar @a_fields > 1 ) ? $a_fields[1] : q{}; $a_key =~ s/^\s+|\s+$//g;
        my $b_key = ( scalar @b_fields > 1 ) ? $b_fields[1] : q{}; $b_key =~ s/^\s+|\s+$//g;
        if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
        if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
        return $b_num <=> $a_num || $b cmp $a;
    }
    my @sort_sorted_0_3 = sort sort_numeric_0_3 @sort_lines_0_3;
    $output_0 = join "\n", @sort_sorted_0_3;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
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

    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print $pipeline1;

print "\nComplex pipeline 2: Text transformation and analysis\n";
print "cat | tr | sed | awk | wc\n";
my $pipeline2 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my $set1_1 = 'a-z';
    my $set2_1 = 'A-Z';
    my $input_1 = $output_0;
    my $expanded_set1_1 = $set1_1;
    my $expanded_set2_1 = $set2_1;
    if ($expanded_set1_1 =~ /a-z/msx) {
        $expanded_set1_1 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    if ($expanded_set1_1 =~ /A-Z/msx) {
        $expanded_set1_1 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    if ($expanded_set2_1 =~ /a-z/msx) {
        $expanded_set2_1 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
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
    my @sed_lines_0 = split /\n/msx, $output_0;
    my @sed_result_0;
    foreach my $line (@sed_lines_0) {
    chomp $line;
    $line =~ s/,/ | /gmsx;
    push @sed_result_0, $line;
    }
    $output_0 = join "\n", @sed_result_0;

    my @lines = split /\n/msx, $output_0;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /\s+/msx, $line;
        push @result, ('Name: ' . $fields[0] . ' | Age: ' . $fields[1] . ' | Role: ' . $fields[2] . ' | Score: ' . $fields[3] . "\n");
    }
    $output_0 = join "", @result;

    use IPC::Open3;
    my @wc_args_0_4 = ('-l');
    my ($wc_in_0_4, $wc_out_0_4, $wc_err_0_4);
    my $wc_pid_0_4 = open3($wc_in_0_4, $wc_out_0_4, $wc_err_0_4, 'wc', @wc_args_0_4);
    print {$wc_in_0_4} $output_0;
    close $wc_in_0_4 or die "Close failed: $!\n";
    $output_0 = do { local $/ = undef; <$wc_out_0_4> };
    if ($output_0 eq q{}) { $output_0 = "0\n"; }
    close $wc_out_0_4 or die "Close failed: $!\n";
    waitpid $wc_pid_0_4, 0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Total processed lines: $pipeline2";

print "\nComplex pipeline 3: Multi-step data analysis\n";
print "cat | cut | sort | uniq -c | sort -nr\n";
my $pipeline3 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my @lines_1 = split /\n/msx, $output_0;
    my @result_1;
    foreach my $line (@lines_1) {
    chomp $line;
    my @fields = split /,/msx, $line;
    if (@fields > 2) {
        push @result_1, $fields[2];
    }
    }
    $output_0 = join "\n", @result_1;

    my @sort_lines_0_2 = split /\n/msx, $output_0;
    my @sort_sorted_0_2 = sort @sort_lines_0_2;
    $output_0 = join "\n", @sort_sorted_0_2;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    my @uniq_lines_0_3 = split /\n/msx, $output_0;
    @uniq_lines_0_3 = grep { $_ ne q{} } @uniq_lines_0_3; 
    my %uniq_counts_0_3;
    foreach my $line (@uniq_lines_0_3) {
    $uniq_counts_0_3{$line}++;
    }
    my @uniq_result_0_3;
    foreach my $line (keys %uniq_counts_0_3) {
    push @uniq_result_0_3, sprintf "%7d %s", $uniq_counts_0_3{$line}, $line;
    }
    $output_0 = join "\n", @uniq_result_0_3;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    my @sort_lines_0_4 = split /\n/msx, $output_0;
    sub sort_numeric_0_4 {
        my @a_fields = split /\s+/msx, $a;
        my @b_fields = split /\s+/msx, $b;
        my $a_num = 0;
        my $b_num = 0;
        my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
        my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
        if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
        if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
        return $b_num <=> $a_num || $b cmp $a;
    }
    my @sort_sorted_0_4 = sort sort_numeric_0_4 @sort_lines_0_4;
    $output_0 = join "\n", @sort_sorted_0_4;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print $pipeline3;

print "\nComplex pipeline 4: File operations and filtering\n";
print "find | grep | xargs | wc\n";
my $pipeline4 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do {
        use File::Find;
        use File::Basename;
        my @files_1 = ();
        my $start_1 = q{.};

        sub find_files_1 {
            my $file_1 = $File::Find::name;
            push @files_1, $file_1;
            return;
        }
        find( \&find_files_1, $start_1 );
        join "\n", @files_1;
    };
    my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /test/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
        if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
            $grep_result_0_1 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    my @xargs_input_0_2 = split /\n/msx, $output_0;
    my @xargs_output_0_2;
    for my $i (0..scalar @xargs_input_0_2-1) {
        my @xargs_args_0_2;
        for my $j (0..1-1) {
            push @xargs_args_0_2, $xargs_input_0_2[$i + $j];
        }
        my ($in_0_2, $out_0_2, $err_0_2);
        my $pid_0_2 = open3($in_0_2, $out_0_2, $err_0_2, 'wc', @xargs_args_0_2);
        close $in_0_2 or croak 'Close failed: $!';
        my $xargs_result_0_2 = do { local $/ = undef; <$out_0_2> };
        close $out_0_2 or croak 'Close failed: $!';
        waitpid $pid_0_2, 0;
        chomp $xargs_result_0_2;
        push @xargs_output_0_2, $xargs_result_0_2;
    }
    my $xargs_result_0_2 = join "\n", @xargs_output_0_2;
    $output_0 = $xargs_result_0_2;

    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print $pipeline4;

print "\nComplex pipeline 5: Data aggregation and formatting\n";
print "cat | awk | sort | head\n";
my $pipeline5 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my @lines = split /\n/msx, $output_0;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /,/msx, $line;
        push @result, ($fields[1] . q{,} . $fields[3] . "\n");
    }
    $output_0 = join "", @result;

    my @sort_lines_0_2 = split /\n/msx, $output_0;
    sub sort_numeric_0_2 {
        my @a_fields = split /,/msx, $a;
        my @b_fields = split /,/msx, $b;
        my $a_num = 0;
        my $b_num = 0;
        my $a_key = ( scalar @a_fields > 1 ) ? $a_fields[1] : q{}; $a_key =~ s/^\s+|\s+$//g;
        my $b_key = ( scalar @b_fields > 1 ) ? $b_fields[1] : q{}; $b_key =~ s/^\s+|\s+$//g;
        if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
        if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
        return $b_num <=> $a_num || $b cmp $a;
    }
    my @sort_sorted_0_2 = sort sort_numeric_0_2 @sort_lines_0_2;
    $output_0 = join "\n", @sort_sorted_0_2;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
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

    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print $pipeline5;

print "\nComplex pipeline 6: Text processing with multiple filters\n";
print "cat | grep | sed | tr | sort | uniq\n";
my $pipeline6 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { !/Manager/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
        if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
            $grep_result_0_1 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    my @sed_lines_0 = split /\n/msx, $output_0;
    my @sed_result_0;
    foreach my $line (@sed_lines_0) {
    chomp $line;
    $line =~ s/,/ /gmsx;
    push @sed_result_0, $line;
    }
    $output_0 = join "\n", @sed_result_0;

    my $set1_1 = 'a-z';
    my $set2_1 = 'A-Z';
    my $input_1 = $output_0;
    my $expanded_set1_1 = $set1_1;
    my $expanded_set2_1 = $set2_1;
    if ($expanded_set1_1 =~ /a-z/msx) {
        $expanded_set1_1 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    if ($expanded_set1_1 =~ /A-Z/msx) {
        $expanded_set1_1 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    if ($expanded_set2_1 =~ /a-z/msx) {
        $expanded_set2_1 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    if ($expanded_set2_1 =~ /A-Z/msx) {
        $expanded_set2_1 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_0_3 = q{};
    for my $char ( split //msx, $input_1 ) {
        my $pos_1 = index $expanded_set1_1, $char;
        if ( $pos_1 >= 0 && $pos_1 < length $expanded_set2_1 ) {
            $tr_result_0_3 .= substr $expanded_set2_1, $pos_1, 1;
        } else {
            $tr_result_0_3 .= $char;
        }
    }
        if (!($tr_result_0_3 =~ m{\n\z}msx || $tr_result_0_3 eq q{})) {
            $tr_result_0_3 .= "\n";
        }
        $output_0 = $tr_result_0_3;
    my @sort_lines_0_4 = split /\n/msx, $output_0;
    my @sort_sorted_0_4 = sort @sort_lines_0_4;
    $output_0 = join "\n", @sort_sorted_0_4;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    my @uniq_lines_0_5 = split /\n/msx, $output_0;
    @uniq_lines_0_5 = grep { $_ ne q{} } @uniq_lines_0_5; 
    my %uniq_seen_0_5;
    my @uniq_result_0_5;
    foreach my $line (@uniq_lines_0_5) {
    if (!$uniq_seen_0_5{$line}++) { push @uniq_result_0_5, $line; }
    }
    $output_0 = join "\n", @uniq_result_0_5;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print $pipeline6;

print "\nComplex pipeline 7: Data validation and reporting\n";
print "cat | awk | grep | wc\n";
my $pipeline7 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my @lines = split /\n/msx, $output_0;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /,/msx, $line;
        if (!($fields[3] > 90)) { next; }
        push @result, ($fields[0] . ' has high score: ' . $fields[3] . "\n");
    }
    $output_0 = join "", @result;

    use IPC::Open3;
    my @wc_args_0_2 = ('-l');
    my ($wc_in_0_2, $wc_out_0_2, $wc_err_0_2);
    my $wc_pid_0_2 = open3($wc_in_0_2, $wc_out_0_2, $wc_err_0_2, 'wc', @wc_args_0_2);
    print {$wc_in_0_2} $output_0;
    close $wc_in_0_2 or die "Close failed: $!\n";
    $output_0 = do { local $/ = undef; <$wc_out_0_2> };
    if ($output_0 eq q{}) { $output_0 = "0\n"; }
    close $wc_out_0_2 or die "Close failed: $!\n";
    waitpid $wc_pid_0_2, 0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "High performers: $pipeline7";

print "\nComplex pipeline 8: File " . "sys" . "tem" . " operations\n";
print "ls | grep | xargs | wc\n";
my $pipeline8 = do { do {
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
    push @ls_files_1, $file;
    }
    closedir $dh;
    @ls_files_1 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_1;
    }
    }
    (@ls_files_1 ? join("\n", @ls_files_1) . "\n" : q{});
    };
    my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /^-/msx } @grep_lines_0_1;
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
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Regular files: $pipeline8";

print "\nComplex pipeline 9: Data transformation and output\n";
print "cat | cut | sort | uniq | tee\n";
my $pipeline9 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my @lines_1 = split /\n/msx, $output_0;
    my @result_1;
    foreach my $line (@lines_1) {
    chomp $line;
    my @fields = split /,/msx, $line;
    if (@fields > 2) {
        push @result_1, $fields[2];
    }
    }
    $output_0 = join "\n", @result_1;

    my @sort_lines_0_2 = split /\n/msx, $output_0;
    my @sort_sorted_0_2 = sort @sort_lines_0_2;
    $output_0 = join "\n", @sort_sorted_0_2;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    my @uniq_lines_0_3 = split /\n/msx, $output_0;
    @uniq_lines_0_3 = grep { $_ ne q{} } @uniq_lines_0_3; 
    my %uniq_seen_0_3;
    my @uniq_result_0_3;
    foreach my $line (@uniq_lines_0_3) {
    if (!$uniq_seen_0_3{$line}++) { push @uniq_result_0_3, $line; }
    }
    $output_0 = join "\n", @uniq_result_0_3;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'roles.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'roles.txt': $!";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Roles saved to file\n";

if (-f "roles.txt") {
    print "Roles file content:\n";
    my $roles_content = do { open my $fh, '<', 'roles.txt' or die 'cat: ' . 'roles.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print $roles_content;
}

print "\nComplex pipeline 10: Error handling and conditional processing\n";
print "cat | grep | awk | sort | head\n";
my $pipeline10 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /Engineer|Developer/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
        if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
            $grep_result_0_1 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    my @lines = split /\n/msx, $output_0;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /,/msx, $line;
        push @result, ($fields[0] . ' (' . $fields[2] . '): ' . $fields[3] . "\n");
    }
    $output_0 = join "", @result;

    my @sort_lines_0_3 = split /\n/msx, $output_0;
    sub sort_numeric_0_3 {
        my @a_fields = split /\s+/msx, $a;
        my @b_fields = split /\s+/msx, $b;
        my $a_num = 0;
        my $b_num = 0;
        my $a_key = ( scalar @a_fields > 2 ) ? $a_fields[2] : q{}; $a_key =~ s/^\s+|\s+$//g;
        my $b_key = ( scalar @b_fields > 2 ) ? $b_fields[2] : q{}; $b_key =~ s/^\s+|\s+$//g;
        if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
        if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
        return $b_num <=> $a_num || $b cmp $a;
    }
    my @sort_sorted_0_3 = sort sort_numeric_0_3 @sort_lines_0_3;
    $output_0 = join "\n", @sort_sorted_0_3;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
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

    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print $pipeline10;

print "\nComplex pipeline 11: Multi-file processing\n";
print "find | xargs | cat | grep | wc\n";
my $pipeline11 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do {
        use File::Find;
        use File::Basename;
        my @files_1 = ();
        my $start_1 = q{.};

        sub find_files_1 {
            my $file_1 = $File::Find::name;
            push @files_1, $file_1;
            return;
        }
        find( \&find_files_1, $start_1 );
        join "\n", @files_1;
    };
    my @xargs_input_0_1 = split /\n/msx, $output_0;
    my @xargs_output_0_1;
    for my $i (0..scalar @xargs_input_0_1-1) {
        my @xargs_args_0_1;
        for my $j (0..1-1) {
            push @xargs_args_0_1, $xargs_input_0_1[$i + $j];
        }
        my ($in_0_1, $out_0_1, $err_0_1);
        my $pid_0_1 = open3($in_0_1, $out_0_1, $err_0_1, 'cat', @xargs_args_0_1);
        close $in_0_1 or croak 'Close failed: $!';
        my $xargs_result_0_1 = do { local $/ = undef; <$out_0_1> };
        close $out_0_1 or croak 'Close failed: $!';
        waitpid $pid_0_1, 0;
        chomp $xargs_result_0_1;
        push @xargs_output_0_1, $xargs_result_0_1;
    }
    my $xargs_result_0_1 = join "\n", @xargs_output_0_1;
    $output_0 = $xargs_result_0_1;

    my $grep_result_0_2;
    my @grep_lines_0_2 = split /\n/msx, $output_0;
    my @grep_filtered_0_2 = grep { /Engineer/msx } @grep_lines_0_2;
    $grep_result_0_2 = scalar @grep_filtered_0_2;
    $CHILD_ERROR = scalar @grep_filtered_0_2 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_2;
    if ((scalar @grep_filtered_0_2) == 0) {
        $pipeline_success_0 = 0;
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Total Engineer mentions: $pipeline11";

print "\nComplex pipeline 12: Data formatting and presentation\n";
print "cat | awk | sort | head | tee\n";
my $pipeline12 = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
    my @lines = split /\n/msx, $output_0;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /,/msx, $line;
        push @result, sprintf("%-10s %3d %-10s %5.1f\n", $fields[0], $fields[1], $fields[2], $fields[3]);
    }
    $output_0 = join "", @result;

    my @sort_lines_0_2 = split /\n/msx, $output_0;
    sub sort_numeric_0_2 {
        my @a_fields = split /\s+/msx, $a;
        my @b_fields = split /\s+/msx, $b;
        my $a_num = 0;
        my $b_num = 0;
        my $a_key = ( scalar @a_fields > 3 ) ? $a_fields[3] : q{}; $a_key =~ s/^\s+|\s+$//g;
        my $b_key = ( scalar @b_fields > 3 ) ? $b_fields[3] : q{}; $b_key =~ s/^\s+|\s+$//g;
        if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
        if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
        return $b_num <=> $a_num || $b cmp $a;
    }
    my @sort_sorted_0_2 = sort sort_numeric_0_2 @sort_lines_0_2;
    $output_0 = join "\n", @sort_sorted_0_2;
        if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
            $output_0 .= "\n";
        }
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

    use Carp qw(carp croak);
    if ( open my $fh, '>', 'top_performers.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'top_performers.txt': $!";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Top performers saved to file\n";

if (-f "top_performers.txt") {
    print "Top performers file content:\n";
    my $top_content = do { open my $fh, '<', 'top_performers.txt' or die 'cat: ' . 'top_performers.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print $top_content;
}

unlink('test_data.txt') if -f 'test_data.txt';
unlink('roles.txt') if -f 'roles.txt';
unlink('top_performers.txt') if -f 'top_performers.txt';

print "=== Example 037 completed successfully ===\n";

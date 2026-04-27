sub __bt { my $s = join('', @_); wantarray ? (split /^/, $s, -1) : $s }
#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/035_pipeline_basic.pl" }


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
my $pipeline_output = __bt(do { do {
    my $output_0 = q{};
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
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
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
);
print $pipeline_output;

print "\nPipeline with multiple commands (cat | grep | wc):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', 'cat test_pipeline.txt | grep a | wc -l'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nPipeline with head and tail:\n";
my $pipeline_head_tail = __bt(do { do {
    my $output_0 = q{};
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

    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
);
print $pipeline_head_tail;

print "\nPipeline with sed and awk:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', q[cat test_pipeline.txt | sed 's/a/A/g' | awk '{print toupper($0)}']); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nPipeline with cut and paste:\n";
my $pipeline_cut_paste = __bt(do { do {
    my $output_0 = q{};
    my $pipeline_success_0 = 1;
    $output_0 .= "1,2,3\n4,5,6\n7,8,9";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines_1 = split /\n/msx, $output_0;
    my @result_1;
    foreach my $line (@lines_1) {
    chomp $line;
    my @fields = split /,/msx, $line;
    my @sel = ();
    if (@fields > 0) { push @sel, $fields[0]; }
    if (@fields > 2) { push @sel, $fields[2]; }
    push @result_1, join(q{,}, @sel);
    }
    $output_0 = join "\n", @result_1;

    $output_0 = do {
        my @paste_stdin_lines_fh_1 = split /\n/msx, $output_0;
        my $paste_output = q{};
        my $stdin_pos_fh_1 = 0;
        for (my $i = 0; ; $i++) {
            my @parts = ();
            my $row_has_data = 0;
            my $val_0_fh_1 = $stdin_pos_fh_1 < scalar @paste_stdin_lines_fh_1 ? $paste_stdin_lines_fh_1[$stdin_pos_fh_1] : q{};
            if ($val_0_fh_1 ne q{}) { $row_has_data = 1; $stdin_pos_fh_1++; }
            push @parts, $val_0_fh_1;
            my $val_1_fh_1 = $stdin_pos_fh_1 < scalar @paste_stdin_lines_fh_1 ? $paste_stdin_lines_fh_1[$stdin_pos_fh_1] : q{};
            if ($val_1_fh_1 ne q{}) { $row_has_data = 1; $stdin_pos_fh_1++; }
            push @parts, $val_1_fh_1;
            last unless $row_has_data;
            $paste_output .= join("\t", @parts) . "\n";
        }
    $paste_output
        };
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
);
print $pipeline_cut_paste;

print "\nPipeline with tr and sort:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', q{cat test_pipeline.txt | tr 'a-z' 'A-Z' | sort}); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nPipeline with uniq and wc:\n";
my $pipeline_uniq_wc = __bt(do { do {
    my $output_0 = q{};
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
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
);
print "Unique lines: $pipeline_uniq_wc";

print "\nPipeline with grep and head:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', 'cat test_pipeline.txt | grep e | head -2'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nPipeline with tail and grep:\n";
my $pipeline_tail_grep = __bt(do { do {
    my $output_0 = q{};
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
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
);
print $pipeline_tail_grep;

print "\nPipeline with multiple filters:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', 'cat test_pipeline.txt | grep a | sort | head -3'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print "\nPipeline with error handling:\n";
my $pipeline_error = __bt(do { do {
    my $output_0 = q{};
    my $pipeline_success_0 = 1;
    $output_0 = do { open my $fh, '<', 'test_pipeline.txt' or die 'cat: ' . 'test_pipeline.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
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
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
);
print "Lines with 'x': $pipeline_error";

print "\nPipeline with tee:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', 'cat test_pipeline.txt | grep a | tee pipeline_output.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

if (-f "pipeline_output.txt") {
    print "Pipeline output file created\n";
    my $output_content = __bt(do { open my $fh, '<', 'pipeline_output.txt' or die 'cat: ' . 'pipeline_output.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
);
    print "Output content: $output_content";
}

unlink('test_pipeline.txt') if -f 'test_pipeline.txt';
unlink('pipeline_output.txt') if -f 'pipeline_output.txt';

print "=== Example 035 completed successfully ===\n";

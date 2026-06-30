#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

my $MAGIC_5 = 5;
my $MAGIC_3 = 3;

print "=== Text Processing Commands ===\n";
my $file_content;
$file_content = do { do {
    my $output_119 = q{};
    my $output_printed_119;
    my $pipeline_success_119 = 1;
    $output_119 = do { open my $fh, '<', 'src/main.rs' or die 'cat: ' . 'src/main.rs' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; };
    my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_119;
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
    $output_119 = $result;

    if ( !$pipeline_success_119 ) { $main_exit_code = 1; }
    if ($output_119 ne q{} && !($output_119 =~ m{\n\z}msx)) {
        $output_119 .= "\n";
    }
    $output_119;
} };
print "First 5 lines of main.rs:\n";
print $file_content;
if ( !( $file_content =~ m{\n\z}msx ) ) { print "\n"; }
my $grep_result;
$grep_result = do { my $grep_result_120;
my @grep_lines_120 = ();
my @grep_filenames_120 = ();
if (-e "src/main.rs") {
    open my $fh, '<', "src/main.rs" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_120, $line;
        push @grep_filenames_120, "src/main.rs";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: src/main.rs: No such file or directory\n"; }
my @grep_filtered_120 = grep { /fn/msx } @grep_lines_120;
my @grep_numbered_120;
for my $i (0..@grep_lines_120-1) {
    if (scalar grep { $_ eq $grep_lines_120[$i] } @grep_filtered_120) {
        push @grep_numbered_120, sprintf "%d:%s", $i + 1, $grep_lines_120[$i];
    }
}
$grep_result_120 = join "\n", @grep_numbered_120;
$CHILD_ERROR = scalar @grep_filtered_120 > 0 ? 0 : 1;
 $grep_result_120; };
print "Lines containing 'fn':\n";
print $grep_result;
if ( !( $grep_result =~ m{\n\z}msx ) ) { print "\n"; }
my $sed_result;
$sed_result = do { do {
    my $output_121 = q{};
    my $output_printed_121;
    my $pipeline_success_121 = 1;
    $output_121 .= 'Hello World' . "\n";
    if ( !($output_121 =~ m{\n\z}msx) ) { $output_121 .= "\n"; }
    $CHILD_ERROR = 0;
    my @sed_lines_121 = split /\n/msx, $output_121;
    my @sed_result_121;
    foreach my $line (@sed_lines_121) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_121, $line;
    }
    $output_121 = join "\n", @sed_result_121;

    if ( !$pipeline_success_121 ) { $main_exit_code = 1; }
    if ($output_121 ne q{} && !($output_121 =~ m{\n\z}msx)) {
        $output_121 .= "\n";
    }
    $output_121;
} };
do {
    my $output = "Sed result: $sed_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $awk_result;
$awk_result = do { do {
    my $output_122 = q{};
    my $output_printed_122;
    my $pipeline_success_122 = 1;
    $output_122 .= '1 2 3 4 5' . "\n";
    if ( !($output_122 =~ m{\n\z}msx) ) { $output_122 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines = split /\n/msx, $output_122;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /\s+/msx, $line;
        push @result, ($fields[0] . $fields[1] . "\n");
    }
    $output_122 = join "", @result;

    if ( !$pipeline_success_122 ) { $main_exit_code = 1; }
    if ($output_122 ne q{} && !($output_122 =~ m{\n\z}msx)) {
        $output_122 .= "\n";
    }
    $output_122;
} };
do {
    my $output = "Awk sum result: $awk_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $sort_result;
$sort_result = do { do {
    my $output_123 = q{};
    my $output_printed_123;
    my $pipeline_success_123 = 1;
    $output_123 .= "zebra\napple\nbanana";
    if ( !($output_123 =~ m{\n\z}msx) ) { $output_123 .= "\n"; }
    $CHILD_ERROR = 0;
    my @sort_lines_123_1 = split /\n/msx, $output_123;
    my @sort_sorted_123_1 = sort @sort_lines_123_1;
    $output_123 = join "\n", @sort_sorted_123_1;
        if ($output_123 ne q{} && !($output_123 =~ m{\n\z}msx)) {
            $output_123 .= "\n";
        }
    if ( !$pipeline_success_123 ) { $main_exit_code = 1; }
    if ($output_123 ne q{} && !($output_123 =~ m{\n\z}msx)) {
        $output_123 .= "\n";
    }
    $output_123;
} };
print "Sorted words:\n";
print $sort_result;
if ( !( $sort_result =~ m{\n\z}msx ) ) { print "\n"; }
my $uniq_result;
$uniq_result = do { do {
    my $output_124 = q{};
    my $output_printed_124;
    my $pipeline_success_124 = 1;
    $output_124 .= "apple\napple\nbanana\nbanana\ncherry";
    if ( !($output_124 =~ m{\n\z}msx) ) { $output_124 .= "\n"; }
    $CHILD_ERROR = 0;
    my @uniq_lines_124_1 = split /\n/msx, $output_124;
    @uniq_lines_124_1 = grep { $_ ne q{} } @uniq_lines_124_1; # Filter out empty lines
    my %uniq_seen_124_1;
    my @uniq_result_124_1;
    foreach my $line (@uniq_lines_124_1) {
    if (!$uniq_seen_124_1{$line}++) { push @uniq_result_124_1, $line; }
    }
    $output_124 = join "\n", @uniq_result_124_1;
        if ($output_124 ne q{} && !($output_124 =~ m{\n\z}msx)) {
            $output_124 .= "\n";
        }
    if ( !$pipeline_success_124 ) { $main_exit_code = 1; }
    if ($output_124 ne q{} && !($output_124 =~ m{\n\z}msx)) {
        $output_124 .= "\n";
    }
    $output_124;
} };
print "Unique words:\n";
print $uniq_result;
if ( !( $uniq_result =~ m{\n\z}msx ) ) { print "\n"; }
my $word_count;
$word_count = do { do {
    my $output_125 = q{};
    my $output_printed_125;
    my $pipeline_success_125 = 1;
    $output_125 .= 'Hello World' . "\n";
    if ( !($output_125 =~ m{\n\z}msx) ) { $output_125 .= "\n"; }
    $CHILD_ERROR = 0;
    use IPC::Open3;
    my @wc_args_125_1 = ('-w');
    my ($wc_in_125_1, $wc_out_125_1, $wc_err_125_1);
    my $wc_pid_125_1 = open3($wc_in_125_1, $wc_out_125_1, $wc_err_125_1, 'wc', @wc_args_125_1);
    print {$wc_in_125_1} $output_125;
    close $wc_in_125_1 or die "Close failed: $!\n";
    $output_125 = do { local $/ = undef; <$wc_out_125_1> };
    close $wc_out_125_1 or die "Close failed: $!\n";
    waitpid $wc_pid_125_1, 0;
    if ( !$pipeline_success_125 ) { $main_exit_code = 1; }
    if ($output_125 ne q{} && !($output_125 =~ m{\n\z}msx)) {
        $output_125 .= "\n";
    }
    $output_125;
} };
my $line_count;
$line_count = do { do {
    my $output_126 = q{};
    my $output_printed_126;
    my $pipeline_success_126 = 1;
    $output_126 .= "line1\nline2\nline3";
    if ( !($output_126 =~ m{\n\z}msx) ) { $output_126 .= "\n"; }
    $CHILD_ERROR = 0;
    use IPC::Open3;
    my @wc_args_126_1 = ('-l');
    my ($wc_in_126_1, $wc_out_126_1, $wc_err_126_1);
    my $wc_pid_126_1 = open3($wc_in_126_1, $wc_out_126_1, $wc_err_126_1, 'wc', @wc_args_126_1);
    print {$wc_in_126_1} $output_126;
    close $wc_in_126_1 or die "Close failed: $!\n";
    $output_126 = do { local $/ = undef; <$wc_out_126_1> };
    if ($output_126 eq q{}) { $output_126 = "0\n"; }
    close $wc_out_126_1 or die "Close failed: $!\n";
    waitpid $wc_pid_126_1, 0;
    if ( !$pipeline_success_126 ) { $main_exit_code = 1; }
    if ($output_126 ne q{} && !($output_126 =~ m{\n\z}msx)) {
        $output_126 .= "\n";
    }
    $output_126;
} };
do {
    my $output = "Word count: $word_count";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = "Line count: $line_count";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $head_result;
$head_result = do { do {
    do { my $output_127 = q{};
my $output_printed_127;
do {
    my $seq_output_128 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_128 = split /\n/msx, $seq_output_128;
    my $output_128 = q{};
    my $head_line_count = 0;
    foreach my $line (@seq_lines_128) {
        chomp $line;
        if ($head_line_count < 3) {
    $output_128 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    }
    $output_128 =~ s/\n+\z//msx;
    $output_128;
} };
} };
do {
    my $output = "First 3 numbers: $head_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $tail_result;
$tail_result = do { do {
    do { my $output_129 = q{};
my $output_printed_129;
do {
    my $seq_output_130 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_130 = split /\n/msx, $seq_output_130;
    my $output_130 = q{};
    my @tail_lines = ();
    foreach my $line (@seq_lines_130) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_130 = join "\n", @last_lines;
        if ($output_130 ne q{}) {
            $output_130 .= "\n";
        }
    }
    $output_130 =~ s/\n+\z//msx;
    $output_130;
} };
} };
do {
    my $output = "Last 3 numbers: $tail_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $cut_result;
$cut_result = do { do {
    my $output_131 = q{};
    my $output_printed_131;
    my $pipeline_success_131 = 1;
    $output_131 .= 'apple:banana:cherry' . "\n";
    if ( !($output_131 =~ m{\n\z}msx) ) { $output_131 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines_132 = split /\n/msx, $output_131;
    my @result_132;
    foreach my $line (@lines_132) {
    chomp $line;
    my @fields = split /:/msx, $line;
    if (@fields > 1) {
        push @result_132, $fields[1];
    }
    }
    $output_131 = join "\n", @result_132;
    if (@result_132 && $output_131 !~ /\n$/) { $output_131 .= "\n"; }
    if ( !$pipeline_success_131 ) { $main_exit_code = 1; }
    if ($output_131 ne q{} && !($output_131 =~ m{\n\z}msx)) {
        $output_131 .= "\n";
    }
    $output_131;
} };
do {
    my $output = "Second field: $cut_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp1.txt'
      or die "Cannot open file: $!\n";
    print "1
2
3\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
    0;
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp2.txt'
      or die "Cannot open file: $!\n";
    print "a
b
c\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
    0;
};
my $paste_result;
$paste_result = do { do {
    my $output_133 = q{};
    my $output_printed_133;
    my $pipeline_success_133 = 1;
    $output_133 = do {
        my @paste_file1_lines_fh_1;
        my @paste_file2_lines_fh_1;
        if (open my $fh1, '<', 'temp1.txt') {
            while (my $line = <$fh1>) {
                chomp $line;
                push @paste_file1_lines_fh_1, $line;
            }
            close $fh1 or croak "Close failed: $OS_ERROR";
        }
        if (open my $fh2, '<', 'temp2.txt') {
            while (my $line = <$fh2>) {
                chomp $line;
                push @paste_file2_lines_fh_1, $line;
            }
            close $fh2 or croak "Close failed: $OS_ERROR";
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
    my @sed_lines_133 = split /\n/msx, $output_133;
    my @sed_result_133;
    foreach my $line (@sed_lines_133) {
    chomp $line;
    $line =~ s/\t/ /gmsx;
    push @sed_result_133, $line;
    }
    $output_133 = join "\n", @sed_result_133;

    if ( !$pipeline_success_133 ) { $main_exit_code = 1; }
    if ($output_133 ne q{} && !($output_133 =~ m{\n\z}msx)) {
        $output_133 .= "\n";
    }
    $output_133;
} };
print "Pasted columns:\n";
print $paste_result;
if ( !( $paste_result =~ m{\n\z}msx ) ) { print "\n"; }
if ( -e "temp1.txt" ) {
    if ( -d "temp1.txt" ) {
        carp "rm: carping: ", "temp1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp1.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "temp1.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    $CHILD_ERROR = 0;
}
if ( -e "temp2.txt" ) {
    if ( -d "temp2.txt" ) {
        carp "rm: carping: ", "temp2.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp2.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "temp2.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    $CHILD_ERROR = 0;
}
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'file1.txt'
      or die "Cannot open file: $!\n";
    print "apple
banana
cherry\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
    0;
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'file2.txt'
      or die "Cannot open file: $!\n";
    print "banana
cherry
date\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
    0;
};
my $comm_result;
$comm_result = do { my @file1_lines;
my @file2_lines;
if (open my $fh1, '<', 'file1.txt') {
    while (my $line = <$fh1>) {
        chomp $line;
        push @file1_lines, $line;
    }
    close $fh1 or croak "Close failed: $OS_ERROR";
}
if (open my $fh2, '<', 'file2.txt') {
    while (my $line = <$fh2>) {
        chomp $line;
        push @file2_lines, $line;
    }
    close $fh2 or croak "Close failed: $OS_ERROR";
}
my %file1_set = map { $_ => 1 } @file1_lines;
my %file2_set = map { $_ => 1 } @file2_lines;
my @common_lines;
foreach my $line (@file1_lines) {
    if (exists $file2_set{$line}) {
        push @common_lines, $line;
    }
}
my $comm_output = q{};
foreach my $line (@common_lines) {
    $comm_output .= $line . "\n";
}
$comm_output =~ s/\n$//msx;
$comm_output };
print "Common lines:\n";
print $comm_result;
if ( !( $comm_result =~ m{\n\z}msx ) ) { print "\n"; }
my $diff_result;
$diff_result = do { my $diff_exit_code = 0;
my $diff_output = q{};
{
    my $diff_cmd = 'diff';
    my @diff_args = ('file1.txt', 'file2.txt');
    my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
    if ($diff_pid) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $diff_output = <$diff_fh>;
        my $close_result = close $diff_fh; # Capture but ignore close result for diff
        $diff_exit_code = $CHILD_ERROR >> 8;
    } else {
        carp "Cannot execute diff command: $OS_ERROR";
        $diff_output = q{};
        $diff_exit_code = 1;
    }
}
$diff_output;
 };
print "File differences:\n";
print $diff_result;
if ( !( $diff_result =~ m{\n\z}msx ) ) { print "\n"; }
my $tr_result;
$tr_result = do { do {
    my $input_data = ("HELLO WORLD") . "\n";
    my $set1_135 = 'A-Z';
my $set2_135 = 'a-z';
my $input_135 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_135 = $set1_135;
my $expanded_set2_135 = $set2_135;
# Handle a-z range in set1
if ($expanded_set1_135 =~ /a-z/msx) {
    $expanded_set1_135 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_135 =~ /A-Z/msx) {
    $expanded_set1_135 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle a-z range in set2
if ($expanded_set2_135 =~ /a-z/msx) {
    $expanded_set2_135 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_135 =~ /A-Z/msx) {
    $expanded_set2_135 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
my $tr_result_134 = q{};
for my $char ( split //msx, $input_135 ) {
    my $pos_135 = index $expanded_set1_135, $char;
    if ( $pos_135 >= 0 && $pos_135 < length $expanded_set2_135 ) {
        $tr_result_134 .= substr $expanded_set2_135, $pos_135, 1;
    } else {
        $tr_result_134 .= $char;
    }
}
$tr_result_134
} };
do {
    my $output = "Lowercase: $tr_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $xargs_result;
$xargs_result = do { do {
    my $output_136 = q{};
    my $output_printed_136;
    my $pipeline_success_136 = 1;
    $output_136 .= '1 2 3' . "\n";
    if ( !($output_136 =~ m{\n\z}msx) ) { $output_136 .= "\n"; }
    $CHILD_ERROR = 0;
    my @xargs_input_136_1 = split /\n/msx, $output_136;
    my @xargs_output_136_1;
    for my $i (0..scalar @xargs_input_136_1-1) {
        my @xargs_args_136_1;
        for my $j (0..1-1) {
            push @xargs_args_136_1, $xargs_input_136_1[$i + $j];
        }
        my ($in_136_1, $out_136_1, $err_136_1);
        my $pid_136_1 = open3($in_136_1, $out_136_1, $err_136_1, '1', 'echo', 'Number:', @xargs_args_136_1);
        close $in_136_1 or croak 'Close failed: $OS_ERROR';
        my $xargs_result_136_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_136_1> };
        close $out_136_1 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_136_1, 0;
        chomp $xargs_result_136_1;
        push @xargs_output_136_1, $xargs_result_136_1;
    }
    my $xargs_result_136_1 = join "\n", @xargs_output_136_1;
    if ($xargs_result_136_1 ne q{} && !( $xargs_result_136_1 =~ m{\n\z}msx )) { $xargs_result_136_1 .= "\n"; }
    $output_136 = $xargs_result_136_1;

    if ( !$pipeline_success_136 ) { $main_exit_code = 1; }
    if ($output_136 ne q{} && !($output_136 =~ m{\n\z}msx)) {
        $output_136 .= "\n";
    }
    $output_136;
} };
print "Xargs result:\n";
print $xargs_result;
if ( !( $xargs_result =~ m{\n\z}msx ) ) { print "\n"; }
if ( -e "file1.txt" ) {
    if ( -d "file1.txt" ) {
        carp "rm: carping: ", "file1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "file1.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "file1.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    $CHILD_ERROR = 0;
}
if ( -e "file2.txt" ) {
    if ( -d "file2.txt" ) {
        carp "rm: carping: ", "file2.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "file2.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "file2.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    $CHILD_ERROR = 0;
}

exit $main_exit_code;

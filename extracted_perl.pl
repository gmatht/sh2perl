
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;

my $MAGIC_5 = 5;
my $MAGIC_3 = 3;

print "=== Text Processing Commands ===\n";
my $file_content = do {
    my $output_0;
    my $pipeline_success_0 = 1;
        $output_0 = q{};
    if (open my $fh, '<', "src/main.rs") {
    while (my $line = <$fh>) {
    $output_0 .= $line;
    }
    close $fh or croak "Close failed: $OS_ERROR";
    # Ensure content ends with newline to prevent line concatenation
        if (!($output_0 =~ /\n$/msx)) {
            $output_0 .= "\n";
        }
    } else {
    carp "cat: ", "src/main.rs", ": No such file or directory";
    $output_0 = q{};
    }
    my $num_lines = 5;
    my $head_line_count = 0;
    my $result = q{};
    my $input = $output_0;
    my $pos = 0;
    while ($pos < length $input && $head_line_count < $num_lines) {
        my $line_end = index $input, "\n", $pos;
        if ($line_end == -1) {
            $line_end = length $input;
        }
        my $head_line = substr $input, $pos, $line_end - $pos;
        $result .= $head_line . "\n";
        $pos = $line_end + 1;
        ++$head_line_count;
    }
    $output_0 = $result;
    if (!$pipeline_success_0) { $main_exit_code = 1; }
        $output_0;
}
;
print "First 5 lines of main.rs:\n";
print $file_content;
if (!($file_content =~ /\n$/msx)) { print "\n"; }
my $grep_result = do { my $grep_result_1;
my @grep_lines_1 = ();
my @grep_filenames_1 = ();
if (-f "src/main.rs") {
    open my $fh, '<', "src/main.rs" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_1, $line;
        push @grep_filenames_1, "src/main.rs";
    }
    close $fh or croak "Close failed: $OS_ERROR";
}
my @grep_filtered_1 = grep { /fn/msx } @grep_lines_1;
my @grep_numbered_1;
for my $i (0..@grep_lines_1-1) {
    if (scalar grep { $_ eq $grep_lines_1[$i] } @grep_filtered_1) {
        push @grep_numbered_1, sprintf "%d:%s", $i + 1, $grep_lines_1[$i];
    }
}
$grep_result_1 = join "\n", @grep_numbered_1;
$CHILD_ERROR = scalar @grep_filtered_1 > 0 ? 0 : 1;
 $grep_result_1; };
print "Lines containing 'fn':\n";
print $grep_result;
if (!($grep_result =~ /\n$/msx)) { print "\n"; }
my $sed_result = do {
    my $output_2;
    my $pipeline_success_2 = 1;
        $output_2 .= "Hello World\n";
    my @sed_lines_2 = split /\n/msx, $output_2;
    my @sed_result_2;
    foreach my $line (@sed_lines_2) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_2, $line;
    }
    $output_2 = join "\n", @sed_result_2;
    if (!$pipeline_success_2) { $main_exit_code = 1; }
        $output_2;
}
;
print "Sed result: $sed_result\n";
my $awk_result = do {
    my $output_3;
    my $pipeline_success_3 = 1;
        $output_3 .= "1 2 3 4 5\n";
    my @lines = split /\n/msx, $output_3;
    my @result;
    foreach my $line (@lines) {
    chomp $line;
    if ($line =~ /^\\s*$/msx) { next; }
    my @fields = split /\s+/msx, $line;
    if (@fields > 0) {
    my $sum = $fields[0] + $fields[1];
    push @result, $sum;
    }
    }
    $output_3 = join "\n", @result;
    if (!$pipeline_success_3) { $main_exit_code = 1; }
        $output_3;
}
;
print "Awk sum result: $awk_result\n";
my $sort_result = do {
    my $output_4;
    my $pipeline_success_4 = 1;
        $output_4 .= "zebra\napple\nbanana";
    my @sort_lines_4_1 = split /\n/msx, $output_4;
    my @sort_sorted_4_1 = sort @sort_lines_4_1;
    $output_4 = join "\n", @sort_sorted_4_1;
        if (!($output_4 =~ /\n$/msx)) {
            $output_4 .= "\n";
        }
    if (!$pipeline_success_4) { $main_exit_code = 1; }
        $output_4;
}
;
print "Sorted words:\n";
print $sort_result;
if (!($sort_result =~ /\n$/msx)) { print "\n"; }
my $uniq_result = do {
    my $output_5;
    my $pipeline_success_5 = 1;
        $output_5 .= "apple\napple\nbanana\nbanana\ncherry";
    my @uniq_lines_5_1 = split /\n/msx, $output_5;
    @uniq_lines_5_1 = grep { $_ ne q{} } @uniq_lines_5_1; # Filter out empty lines
    my %uniq_seen_5_1;
    my @uniq_result_5_1;
    foreach my $line (@uniq_lines_5_1) {
    if (!$uniq_seen_5_1{$line}++) { push @uniq_result_5_1, $line; }
    }
    $output_5 = join "\n", @uniq_result_5_1;
        if (!($output_5 =~ /\n$/msx)) {
            $output_5 .= "\n";
        }
    if (!$pipeline_success_5) { $main_exit_code = 1; }
        $output_5;
}
;
print "Unique words:\n";
print $uniq_result;
if (!($uniq_result =~ /\n$/msx)) { print "\n"; }
my $word_count = do {
    my $output_6;
    my $pipeline_success_6 = 1;
        $output_6 .= "Hello World\n";
    my @wc_lines_6_1 = split /\n/msx, $output_6;
    my $wc_word_count_6_1 = 0;
    foreach my $line (@wc_lines_6_1) {
        my @wc_words_6_1 = split /\s+/msx, $line;
        $wc_word_count_6_1 += scalar @wc_words_6_1;
    }
    $output_6 = q{};
    $output_6 .= "$wc_word_count_6_1 ";
    $output_6 =~ s/\s+$//msx;
    if (!$pipeline_success_6) { $main_exit_code = 1; }
        $output_6;
}
;
my $line_count = do {
    my $output_7;
    my $pipeline_success_7 = 1;
        $output_7 .= "line1\nline2\nline3";
    my @wc_lines_7_1 = split /\n/msx, $output_7;
    my $wc_line_count_7_1 = scalar @wc_lines_7_1;
    $output_7 = q{};
    $output_7 .= "$wc_line_count_7_1 ";
    $output_7 =~ s/\s+$//msx;
    if (!$pipeline_success_7) { $main_exit_code = 1; }
        $output_7;
}
;
print "Word count: $word_count\n";
print "Line count: $line_count\n";
my $head_result = do {
    my $seq_output_9 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_9 = split /\n/msx, $seq_output_9;
    my $output_9 = q{};
    my $head_line_count = 0;
    foreach my $line (@seq_lines_9) {
        chomp $line;
        if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_9 .= "\n"; }
    $output_9 .= $line;
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    }
    $output_9;
}
;
print "First 3 numbers: $head_result\n";
my $tail_result = do {
    my $seq_output_11 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_11 = split /\n/msx, $seq_output_11;
    my $output_11 = q{};
    my @tail_lines = ();
    foreach my $line (@seq_lines_11) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_11 = join "\n", @last_lines;
        if ($output_11 ne q{}) {
            $output_11 .= "\n";
        }
    }
    $output_11;
}
;
print "Last 3 numbers: $tail_result\n";
my $cut_result = do {
    my $output_12;
    my $pipeline_success_12 = 1;
        $output_12 .= "apple:banana:cherry\n";
    my @lines_13 = split /\n/msx, $output_12;
    my @result_13;
    foreach my $line (@lines_13) {
    chomp $line;
    my @fields = split /\:/msx, $line;
    if (@fields > 1) {
    push @result_13, $fields[1];
    }
    }
    $output_12 = join "\n", @result_13;
    if (!$pipeline_success_12) { $main_exit_code = 1; }
        $output_12;
}
;
print "Second field: $cut_result\n";
{
    open my $original_stdout, '>&', STDOUT
    or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'temp1.txt'
    or croak "Cannot open file: $ERRNO";
    print "1\n2\n3\n";
    open STDOUT, '>&', $original_stdout
    or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
{
    open my $original_stdout, '>&', STDOUT
    or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'temp2.txt'
    or croak "Cannot open file: $ERRNO";
    print "a\nb\nc\n";
    open STDOUT, '>&', $original_stdout
    or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $paste_result = do {
    my $output_14;
    my $pipeline_success_14 = 1;
        do {
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
        }
    my @sed_lines_14 = split /\n/msx, $output_14;
    my @sed_result_14;
    foreach my $line (@sed_lines_14) {
    chomp $line;
    $line =~ s/\t/ /gmsx;
    push @sed_result_14, $line;
    }
    $output_14 = join "\n", @sed_result_14;
    if (!$pipeline_success_14) { $main_exit_code = 1; }
        $output_14;
}
;
print "Pasted columns:\n";
print $paste_result;
if (!($paste_result =~ /\n$/msx)) { print "\n"; }
if (-e "temp1.txt") {
if (-d "temp1.txt") {
carp "rm: carping: ", "temp1.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "temp1.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "temp1.txt",
    ": $OS_ERROR\n";
}
}
} else {
local $CHILD_ERROR = 0;
carp "rm: carping: ", "temp1.txt",
    ": No such file or directory\n";
}
if (-e "temp2.txt") {
if (-d "temp2.txt") {
carp "rm: carping: ", "temp2.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "temp2.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "temp2.txt",
    ": $OS_ERROR\n";
}
}
} else {
local $CHILD_ERROR = 0;
carp "rm: carping: ", "temp2.txt",
    ": No such file or directory\n";
}
{
    open my $original_stdout, '>&', STDOUT
    or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'file1.txt'
    or croak "Cannot open file: $ERRNO";
    print "apple\nbanana\ncherry\n";
    open STDOUT, '>&', $original_stdout
    or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
{
    open my $original_stdout, '>&', STDOUT
    or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'file2.txt'
    or croak "Cannot open file: $ERRNO";
    print "banana\ncherry\ndate\n";
    open STDOUT, '>&', $original_stdout
    or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $comm_result = do { my @file1_lines;
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
if (!($comm_result =~ /\n$/msx)) { print "\n"; }
my $diff_result = do { my $diff_exit_code = 0;
my $diff_output = q{};
{
    my $diff_cmd = 'diff.exe';
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
$diff_output };
print "File differences:\n";
print $diff_result;
if (!($diff_result =~ /\n$/msx)) { print "\n"; }
my $tr_result = do {
    my $input_data = ("HELLO WORLD");
    my $set1_16 = 'A-Z';
my $set2_16 = 'a-z';
my $input_16 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_16 = $set1_16;
my $expanded_set2_16 = $set2_16;
# Handle a-z range in set1
if ($expanded_set1_16 =~ /a-z/msx) {
    $expanded_set1_16 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_16 =~ /A-Z/msx) {
    $expanded_set1_16 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle a-z range in set2
if ($expanded_set2_16 =~ /a-z/msx) {
    $expanded_set2_16 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_16 =~ /A-Z/msx) {
    $expanded_set2_16 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
my $tr_result_15 = q{};
for my $char ( split //msx, $input_16 ) {
    my $pos_16 = index $expanded_set1_16, $char;
    if ( $pos_16 >= 0 && $pos_16 < length $expanded_set2_16 ) {
        $tr_result_15 .= substr $expanded_set2_16, $pos_16, 1;
    } else {
        $tr_result_15 .= $char;
    }
}
$tr_result_15
};
print "Lowercase: $tr_result\n";
my $xargs_result = do {
    my $output_17;
    my $pipeline_success_17 = 1;
        $output_17 .= "1 2 3\n";
    my @xargs_input_17_1 = split /\s+/msx, $output_17;
    my @xargs_output_17_1;
    for my $i (0..scalar @xargs_input_17_1-1) {
        my @xargs_args_17_1;
        for my $j (0..1-1) {
            push @xargs_args_17_1, $xargs_input_17_1[$i + $j];
        }
        my $xargs_line_17_1 = q{};
        $xargs_line_17_1 .= "Number:";
        foreach my $arg (@xargs_args_17_1) {
            $xargs_line_17_1 .= q{ } . $arg;
        }
        push @xargs_output_17_1, $xargs_line_17_1;
    }
    my $xargs_result_17_1 = join "\n", @xargs_output_17_1;
    $output_17 = $xargs_result_17_1;
    if (!$pipeline_success_17) { $main_exit_code = 1; }
        $output_17;
}
;
print "Xargs result:\n";
print $xargs_result;
if (!($xargs_result =~ /\n$/msx)) { print "\n"; }
if (-e "file1.txt") {
if (-d "file1.txt") {
carp "rm: carping: ", "file1.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "file1.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "file1.txt",
    ": $OS_ERROR\n";
}
}
} else {
local $CHILD_ERROR = 0;
carp "rm: carping: ", "file1.txt",
    ": No such file or directory\n";
}
if (-e "file2.txt") {
if (-d "file2.txt") {
carp "rm: carping: ", "file2.txt",
    " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "file2.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "file2.txt",
    ": $OS_ERROR\n";
}
}
} else {
local $CHILD_ERROR = 0;
carp "rm: carping: ", "file2.txt",
    ": No such file or directory\n";
}



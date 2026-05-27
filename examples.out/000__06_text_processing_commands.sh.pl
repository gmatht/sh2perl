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
my $__set_e        = 0;
our $CHILD_ERROR;

my $MAGIC_5 = 5;
my $MAGIC_3 = 3;

print "=== Text Processing Commands ===\n";
my $file_content = do { do {
    my $output_117 = q{};
    my $output_printed_117;
    my $pipeline_success_117 = 1;
    $output_117 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'src/main.rs' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'src/main.rs' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_117;
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
    $output_117 = $result;

    if ( !$pipeline_success_117 ) { $main_exit_code = 1; }
    $output_117 =~ s/\n+\z//msx;
    $output_117;
} };
print "First 5 lines of main.rs:\n";
print $file_content;
if ( !( $file_content =~ m{\n\z}msx ) ) { print "\n"; }
my $grep_result = do { my $grep_result_118;
my @grep_lines_118 = ();
my @grep_filenames_118 = ();
if (-e "src/main.rs") {
    open my $fh, '<', "src/main.rs" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_118, $line;
        push @grep_filenames_118, "src/main.rs";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: src/main.rs: No such file or directory\n"; }
my @grep_filtered_118 = grep { /fn/msx } @grep_lines_118;
my @grep_numbered_118;
for my $i (0..@grep_lines_118-1) {
    if (scalar grep { $_ eq $grep_lines_118[$i] } @grep_filtered_118) {
        push @grep_numbered_118, sprintf "%d:%s", $i + 1, $grep_lines_118[$i];
    }
}
$grep_result_118 = join "\n", @grep_numbered_118;
$CHILD_ERROR = scalar @grep_filtered_118 > 0 ? 0 : 1;
 $grep_result_118; };
print "Lines containing 'fn':\n";
print $grep_result;
if ( !( $grep_result =~ m{\n\z}msx ) ) { print "\n"; }
my $sed_result = do { do {
    my $output_119 = q{};
    my $output_printed_119;
    my $pipeline_success_119 = 1;
    $output_119 .= 'Hello World' . "\n";
    if ( !($output_119 =~ m{\n\z}msx) ) { $output_119 .= "\n"; }
    $CHILD_ERROR = 0;
    my @sed_lines_119 = split /\n/msx, $output_119;
    my @sed_result_119;
    foreach my $line (@sed_lines_119) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_119, $line;
    }
    $output_119 = join "\n", @sed_result_119;

    if ( !$pipeline_success_119 ) { $main_exit_code = 1; }
    $output_119 =~ s/\n+\z//msx;
    $output_119;
} };
do {
    my $output = "Sed result: $sed_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $awk_result = do { do {
    my $output_120 = q{};
    my $output_printed_120;
    my $pipeline_success_120 = 1;
    $output_120 .= '1 2 3 4 5' . "\n";
    if ( !($output_120 =~ m{\n\z}msx) ) { $output_120 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines = split /\n/msx, $output_120;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /\s+/msx, $line;
        push @result, ($fields[0] + $fields[1] . "\n");
    }
    $output_120 = join "", @result;

    if ( !$pipeline_success_120 ) { $main_exit_code = 1; }
    $output_120 =~ s/\n+\z//msx;
    $output_120;
} };
do {
    my $output = "Awk sum result: $awk_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $sort_result = do { do {
    my $output_121 = q{};
    my $output_printed_121;
    my $pipeline_success_121 = 1;
    $output_121 .= "zebra\napple\nbanana";
    if ( !($output_121 =~ m{\n\z}msx) ) { $output_121 .= "\n"; }
    $CHILD_ERROR = 0;
    my @sort_lines_121_1 = split /\n/msx, $output_121;
    my @sort_sorted_121_1 = sort @sort_lines_121_1;
    $output_121 = join "\n", @sort_sorted_121_1;
        if ($output_121 ne q{} && !($output_121 =~ m{\n\z}msx)) {
            $output_121 .= "\n";
        }
    if ( !$pipeline_success_121 ) { $main_exit_code = 1; }
    $output_121 =~ s/\n+\z//msx;
    $output_121;
} };
print "Sorted words:\n";
print $sort_result;
if ( !( $sort_result =~ m{\n\z}msx ) ) { print "\n"; }
my $uniq_result = do { do {
    my $output_122 = q{};
    my $output_printed_122;
    my $pipeline_success_122 = 1;
    $output_122 .= "apple\napple\nbanana\nbanana\ncherry";
    if ( !($output_122 =~ m{\n\z}msx) ) { $output_122 .= "\n"; }
    $CHILD_ERROR = 0;
    my @uniq_lines_122_1 = split /\n/msx, $output_122;
    @uniq_lines_122_1 = grep { $_ ne q{} } @uniq_lines_122_1; # Filter out empty lines
    my %uniq_seen_122_1;
    my @uniq_result_122_1;
    foreach my $line (@uniq_lines_122_1) {
    if (!$uniq_seen_122_1{$line}++) { push @uniq_result_122_1, $line; }
    }
    $output_122 = join "\n", @uniq_result_122_1;
        if ($output_122 ne q{} && !($output_122 =~ m{\n\z}msx)) {
            $output_122 .= "\n";
        }
    if ( !$pipeline_success_122 ) { $main_exit_code = 1; }
    $output_122 =~ s/\n+\z//msx;
    $output_122;
} };
print "Unique words:\n";
print $uniq_result;
if ( !( $uniq_result =~ m{\n\z}msx ) ) { print "\n"; }
my $word_count = do { do {
    my $output_123 = q{};
    my $output_printed_123;
    my $pipeline_success_123 = 1;
    $output_123 .= 'Hello World' . "\n";
    if ( !($output_123 =~ m{\n\z}msx) ) { $output_123 .= "\n"; }
    $CHILD_ERROR = 0;
    use IPC::Open3;
    my @wc_args_123_1 = ('-w');
    my ($wc_in_123_1, $wc_out_123_1, $wc_err_123_1);
    my $wc_pid_123_1 = open3($wc_in_123_1, $wc_out_123_1, $wc_err_123_1, 'wc', @wc_args_123_1);
    print {$wc_in_123_1} $output_123;
    close $wc_in_123_1 or die "Close failed: $OS_ERROR\n";
    $output_123 = do { local $/ = undef; <$wc_out_123_1> };
    close $wc_out_123_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_123_1, 0;
    if ( !$pipeline_success_123 ) { $main_exit_code = 1; }
    $output_123 =~ s/\n+\z//msx;
    $output_123;
} };
my $line_count = do { do {
    my $output_124 = q{};
    my $output_printed_124;
    my $pipeline_success_124 = 1;
    $output_124 .= "line1\nline2\nline3";
    if ( !($output_124 =~ m{\n\z}msx) ) { $output_124 .= "\n"; }
    $CHILD_ERROR = 0;
    use IPC::Open3;
    my @wc_args_124_1 = ('-l');
    my ($wc_in_124_1, $wc_out_124_1, $wc_err_124_1);
    my $wc_pid_124_1 = open3($wc_in_124_1, $wc_out_124_1, $wc_err_124_1, 'wc', @wc_args_124_1);
    print {$wc_in_124_1} $output_124;
    close $wc_in_124_1 or die "Close failed: $OS_ERROR\n";
    $output_124 = do { local $/ = undef; <$wc_out_124_1> };
    if ($output_124 eq q{}) { $output_124 = "0\n"; }
    close $wc_out_124_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_124_1, 0;
    if ( !$pipeline_success_124 ) { $main_exit_code = 1; }
    $output_124 =~ s/\n+\z//msx;
    $output_124;
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
my $head_result = do { do {
    do { my $output_125 = q{};
my $output_printed_125;
do {
    my $seq_output_126 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_126 = split /\n/msx, $seq_output_126;
    my $output_126 = q{};
    my $head_line_count = 0;
    foreach my $line (@seq_lines_126) {
        chomp $line;
        if ($head_line_count < 3) {
    $output_126 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    }
    $output_126 =~ s/\n+\z//msx;
    $output_126;
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
my $tail_result = do { do {
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
    my @tail_lines = ();
    foreach my $line (@seq_lines_128) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_128 = join "\n", @last_lines;
        if ($output_128 ne q{}) {
            $output_128 .= "\n";
        }
    }
    $output_128 =~ s/\n+\z//msx;
    $output_128;
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
my $cut_result = do { do {
    my $output_129 = q{};
    my $output_printed_129;
    my $pipeline_success_129 = 1;
    $output_129 .= 'apple:banana:cherry' . "\n";
    if ( !($output_129 =~ m{\n\z}msx) ) { $output_129 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines_130 = split /\n/msx, $output_129;
    my @result_130;
    foreach my $line (@lines_130) {
    chomp $line;
    my @fields = split /:/msx, $line;
    if (@fields > 1) {
        push @result_130, $fields[1];
    }
    }
    $output_129 = join "\n", @result_130;

    if ( !$pipeline_success_129 ) { $main_exit_code = 1; }
    $output_129 =~ s/\n+\z//msx;
    $output_129;
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
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp1.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "1
2
3\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp2.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "a
b
c\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $paste_result = do { do {
    my $output_131 = q{};
    my $output_printed_131;
    my $pipeline_success_131 = 1;
    $output_131 = do {
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
    my @sed_lines_131 = split /\n/msx, $output_131;
    my @sed_result_131;
    foreach my $line (@sed_lines_131) {
    chomp $line;
    $line =~ s/\t/ /gmsx;
    push @sed_result_131, $line;
    }
    $output_131 = join "\n", @sed_result_131;

    if ( !$pipeline_success_131 ) { $main_exit_code = 1; }
    $output_131 =~ s/\n+\z//msx;
    $output_131;
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
    local $CHILD_ERROR = 0;
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
    local $CHILD_ERROR = 0;
}
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file1.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "apple
banana
cherry\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file2.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "banana
cherry
date\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
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
if ( !( $comm_result =~ m{\n\z}msx ) ) { print "\n"; }
my $diff_result = do { my $diff_exit_code = 0;
my $diff_output = q{};
{
    my $diff_cmd = 'diff';
    my @diff_args = ('file1.txt', 'file2.txt');
    my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
    if ($diff_pid) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $diff_output = <$diff_fh>;
        close $diff_fh;
        $diff_exit_code = $? >> 8;
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
my $tr_result = do { do {
    my $input_data = ("HELLO WORLD") . "\n";
    my $set1_133 = 'A-Z';
my $set2_133 = 'a-z';
my $input_133 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_133 = $set1_133;
my $expanded_set2_133 = $set2_133;
# Handle a-z range in set1
if ($expanded_set1_133 =~ /a-z/msx) {
    $expanded_set1_133 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_133 =~ /A-Z/msx) {
    $expanded_set1_133 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle a-z range in set2
if ($expanded_set2_133 =~ /a-z/msx) {
    $expanded_set2_133 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_133 =~ /A-Z/msx) {
    $expanded_set2_133 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
my $tr_result_132 = q{};
for my $char ( split //msx, $input_133 ) {
    my $pos_133 = index $expanded_set1_133, $char;
    if ( $pos_133 >= 0 && $pos_133 < length $expanded_set2_133 ) {
        $tr_result_132 .= substr $expanded_set2_133, $pos_133, 1;
    } else {
        $tr_result_132 .= $char;
    }
}
$tr_result_132
} };
do {
    my $output = "Lowercase: $tr_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $xargs_result = do { do {
    my $output_134 = q{};
    my $output_printed_134;
    my $pipeline_success_134 = 1;
    $output_134 .= '1 2 3' . "\n";
    if ( !($output_134 =~ m{\n\z}msx) ) { $output_134 .= "\n"; }
    $CHILD_ERROR = 0;
    my @xargs_input_134_1 = grep { $_ ne q{} } split /\s+/msx, $output_134;
    my @xargs_output_134_1;
    for my $i (0..scalar @xargs_input_134_1-1) {
        my @xargs_args_134_1;
        for my $j (0..1-1) {
            push @xargs_args_134_1, $xargs_input_134_1[$i + $j];
        }
        my $xargs_line_134_1 = q{};
        $xargs_line_134_1 .= "Number:";
        foreach my $arg (@xargs_args_134_1) {
            $xargs_line_134_1 .= q{ } . $arg;
        }
        push @xargs_output_134_1, $xargs_line_134_1;
    }
    my $xargs_result_134_1 = join "\n", @xargs_output_134_1;
    if ($xargs_result_134_1 ne q{} && !( $xargs_result_134_1 =~ m{\n\z}msx )) { $xargs_result_134_1 .= "\n"; }
    $output_134 = $xargs_result_134_1;

    if ( !$pipeline_success_134 ) { $main_exit_code = 1; }
    $output_134 =~ s/\n+\z//msx;
    $output_134;
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
    local $CHILD_ERROR = 0;
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
    local $CHILD_ERROR = 0;
}

exit $main_exit_code;

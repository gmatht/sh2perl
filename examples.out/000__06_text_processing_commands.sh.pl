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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__06_text_processing_commands.sh';
my $MAGIC_5 = 5;
my $MAGIC_3 = 3;

print "=== Text Processing Commands ===\n";
my $file_content;
my @file_content;
my %file_content;
$file_content = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_118 = q{};
    my $output_printed_118;
    my $pipeline_success_118 = 1;
    $output_118 = do { my $cat_chunk = q{}; if ( open my $fh, '<', 'src/main.rs' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . 'src/main.rs' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($CHILD_ERROR != 0) { $pipeline_success_118 = 0; }
    my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_118;
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
    $output_118 = $result;

    if ( !$pipeline_success_118 ) { $main_exit_code = 1; }
    $output_118 =~ s/\n+\z//msx;
    $output_118;
}; $_pipeline_result; };
print "First 5 lines of main.rs:\n";
print $file_content;
if ( !( ($file_content) =~ m{\n\z}msx ) ) { print "\n"; }
my $grep_result;
my @grep_result;
my %grep_result;
$grep_result = do { my $grep_result_119;
my @grep_lines_119 = ();
my @grep_filenames_119 = ();
if (-e "src/main.rs") {
    open my $fh, '<', "src/main.rs" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_119, $line;
        push @grep_filenames_119, "src/main.rs";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: src/main.rs: No such file or directory\n"; }
my @grep_filtered_119 = grep { /fn/msx } @grep_lines_119;
my @grep_numbered_119;
for my $i (0..@grep_lines_119-1) {
    if (scalar grep { $_ eq $grep_lines_119[$i] } @grep_filtered_119) {
        push @grep_numbered_119, sprintf "%d:%s", $i + 1, $grep_lines_119[$i];
    }
}
$grep_result_119 = join "\n", @grep_numbered_119;
$CHILD_ERROR = scalar @grep_filtered_119 > 0 ? 0 : 1;
 $grep_result_119; };
print "Lines containing 'fn':\n";
print $grep_result;
if ( !( ($grep_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $sed_result;
my @sed_result;
my %sed_result;
$sed_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_120 = q{};
    my $output_printed_120;
    my $pipeline_success_120 = 1;
    $output_120 .= 'Hello World' . "\n";
    if ( !($output_120 =~ m{\n\z}msx) ) { $output_120 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_120 = 0; }
    my @sed_lines_120 = split /\n/msx, $output_120;
    my @sed_result_120;
    foreach my $line (@sed_lines_120) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_120, $line;
    }
    $output_120 = join "\n", @sed_result_120;

    if ( !$pipeline_success_120 ) { $main_exit_code = 1; }
    $output_120 =~ s/\n+\z//msx;
    $output_120;
}; $_pipeline_result; };
do {
    my $__echo_line = "Sed result: $sed_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $awk_result;
my @awk_result;
my %awk_result;
$awk_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_121 = q{};
    my $output_printed_121;
    my $pipeline_success_121 = 1;
    $output_121 .= '1 2 3 4 5' . "\n";
    if ( !($output_121 =~ m{\n\z}msx) ) { $output_121 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_121 = 0; }
    my @lines = split /\n/msx, $output_121;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /\s+/msx, $line;
        push @result, ($fields[0] + $fields[1] . "\n");
    }
    $output_121 = join "", @result;

    if ( !$pipeline_success_121 ) { $main_exit_code = 1; }
    $output_121 =~ s/\n+\z//msx;
    $output_121;
}; $_pipeline_result; };
do {
    my $__echo_line = "Awk sum result: $awk_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $sort_result;
my @sort_result;
my %sort_result;
$sort_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_122 = q{};
    my $output_printed_122;
    my $pipeline_success_122 = 1;
    $output_122 .= "zebra\napple\nbanana";
    if ( !($output_122 =~ m{\n\z}msx) ) { $output_122 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_122 = 0; }
    my @sort_lines_122_1 = split /\n/msx, $output_122;
    my @sort_sorted_122_1 = sort @sort_lines_122_1;
    $output_122 = join "\n", @sort_sorted_122_1;
        if ($output_122 ne q{} && !($output_122 =~ m{\n\z}msx)) {
            $output_122 .= "\n";
        }
    if ( !$pipeline_success_122 ) { $main_exit_code = 1; }
    $output_122 =~ s/\n+\z//msx;
    $output_122;
}; $_pipeline_result; };
print "Sorted words:\n";
print $sort_result;
if ( !( ($sort_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $uniq_result;
my @uniq_result;
my %uniq_result;
$uniq_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_123 = q{};
    my $output_printed_123;
    my $pipeline_success_123 = 1;
    $output_123 .= "apple\napple\nbanana\nbanana\ncherry";
    if ( !($output_123 =~ m{\n\z}msx) ) { $output_123 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_123 = 0; }
    my @uniq_lines_123_1 = split /\n/msx, $output_123;
    @uniq_lines_123_1 = grep { $_ ne q{} } @uniq_lines_123_1; # Filter out empty lines
    my %uniq_seen_123_1;
    my @uniq_result_123_1;
    foreach my $line (@uniq_lines_123_1) {
    if (!$uniq_seen_123_1{$line}++) { push @uniq_result_123_1, $line; }
    }
    $output_123 = join "\n", @uniq_result_123_1;
        if ($output_123 ne q{} && !($output_123 =~ m{\n\z}msx)) {
            $output_123 .= "\n";
        }
    if ( !$pipeline_success_123 ) { $main_exit_code = 1; }
    $output_123 =~ s/\n+\z//msx;
    $output_123;
}; $_pipeline_result; };
print "Unique words:\n";
print $uniq_result;
if ( !( ($uniq_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $word_count;
my @word_count;
my %word_count;
$word_count = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_124 = q{};
    my $output_printed_124;
    my $pipeline_success_124 = 1;
    $output_124 .= 'Hello World' . "\n";
    if ( !($output_124 =~ m{\n\z}msx) ) { $output_124 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_124 = 0; }
    $output_124 = do {
            my $_wc_data = $output_124;
            my $_wc_words = scalar split /\s+/msx, $_wc_data;
            my $_wc_result = q{};
            $_wc_result .= sprintf q{%d}, $_wc_words;
            $_wc_result .= "\n";
            $_wc_result;
        };
    if ( !$pipeline_success_124 ) { $main_exit_code = 1; }
    $output_124 =~ s/\n+\z//msx;
    $output_124;
}; $_pipeline_result; };
my $line_count;
my @line_count;
my %line_count;
$line_count = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_125 = q{};
    my $output_printed_125;
    my $pipeline_success_125 = 1;
    $output_125 .= "line1\nline2\nline3";
    if ( !($output_125 =~ m{\n\z}msx) ) { $output_125 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_125 = 0; }
    $output_125 = do {
            my $_wc_data = $output_125;
            my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
            my $_wc_result = q{};
            $_wc_result .= sprintf q{%d}, $_wc_lines;
            $_wc_result .= "\n";
            $_wc_result;
        };
    if ( !$pipeline_success_125 ) { $main_exit_code = 1; }
    $output_125 =~ s/\n+\z//msx;
    $output_125;
}; $_pipeline_result; };
do {
    my $__echo_line = "Word count: $word_count";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = "Line count: $line_count";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $head_result;
my @head_result;
my %head_result;
$head_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    do { my $output_126 = q{};
my $output_printed_126;
do {
    my $seq_output_127 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_127 = split /\n/msx, $seq_output_127;
    my $output_127 = q{};
    my $head_line_count = 0;
    foreach my $line (@seq_lines_127) {
        chomp $line;
        if ($head_line_count < 3) {
    $output_127 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    }
    $output_127;
} };
}; $_pipeline_result; };
do {
    my $__echo_line = "First 3 numbers: $head_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $tail_result;
my @tail_result;
my %tail_result;
$tail_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    do { my $output_128 = q{};
my $output_printed_128;
do {
    my $seq_output_129 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_129 = split /\n/msx, $seq_output_129;
    my $output_129 = q{};
    my @tail_lines = ();
    foreach my $line (@seq_lines_129) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_129 = join "\n", @last_lines;
        if ($output_129 ne q{}) {
            $output_129 .= "\n";
        }
    }
    $output_129;
} };
}; $_pipeline_result; };
do {
    my $__echo_line = "Last 3 numbers: $tail_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $cut_result;
my @cut_result;
my %cut_result;
$cut_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_130 = q{};
    my $output_printed_130;
    my $pipeline_success_130 = 1;
    $output_130 .= 'apple:banana:cherry' . "\n";
    if ( !($output_130 =~ m{\n\z}msx) ) { $output_130 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_130 = 0; }
    my @lines_131 = split /\n/msx, $output_130;
    my @result_131;
    foreach my $line (@lines_131) {
    chomp $line;
    my @fields = split /:/msx, $line;
    if (@fields > 1) {
        push @result_131, $fields[1];
    }
    }
    $output_130 = join "\n", @result_131;
    if ($output_130 ne q{} && !($output_130  =~ m{\n\z}msx)) { $output_130 .= "\n"; }

    if ( !$pipeline_success_130 ) { $main_exit_code = 1; }
    $output_130 =~ s/\n+\z//msx;
    $output_130;
}; $_pipeline_result; };
do {
    my $__echo_line = "Second field: $cut_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp1.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "1\n2\n3" . "\n";
    $CHILD_ERROR = 0;
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
    print "a\nb\nc" . "\n";
    $CHILD_ERROR = 0;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $paste_result;
my @paste_result;
my %paste_result;
$paste_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_132 = q{};
    my $output_printed_132;
    my $pipeline_success_132 = 1;
    $output_132 = do {
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
    ;
    if ($CHILD_ERROR != 0) { $pipeline_success_132 = 0; }
    my @sed_lines_132 = split /\n/msx, $output_132;
    my @sed_result_132;
    foreach my $line (@sed_lines_132) {
    chomp $line;
    $line =~ s/\t/ /gmsx;
    push @sed_result_132, $line;
    }
    $output_132 = join "\n", @sed_result_132;

    if ( !$pipeline_success_132 ) { $main_exit_code = 1; }
    $output_132 =~ s/\n+\z//msx;
    $output_132;
}; $_pipeline_result; };
print "Pasted columns:\n";
print $paste_result;
if ( !( ($paste_result) =~ m{\n\z}msx ) ) { print "\n"; }
if ( -e "temp1.txt" ) {
    if ( -d "temp1.txt" ) {
        carp "rm: carping: ", "temp1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp1.txt" ) {
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
    print "apple\nbanana\ncherry" . "\n";
    $CHILD_ERROR = 0;
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
    print "banana\ncherry\ndate" . "\n";
    $CHILD_ERROR = 0;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $comm_result;
my @comm_result;
my %comm_result;
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
if ( !( ($comm_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $diff_result;
my @diff_result;
my %diff_result;
$diff_result = do { my $diff_output = q{};
{
    my $diff_cmd = 'diff';
    my @diff_args = ('file1.txt', 'file2.txt');
    my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
    if ($diff_pid) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $diff_output = <$diff_fh>;
        close $diff_fh;
        $CHILD_ERROR = $? >> 8;
    } else {
        carp "Cannot execute diff command: $OS_ERROR";
        $diff_output = q{};
        $CHILD_ERROR = 1;
    }
}
$diff_output;
 };
print "File differences:\n";
print $diff_result;
if ( !( ($diff_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $tr_result;
my @tr_result;
my %tr_result;
$tr_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $input_data = ("HELLO WORLD") . "\n";
    my $set1_134 = 'A-Z';
my $set2_134 = 'a-z';
my $input_134 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_134 = $set1_134;
my $expanded_set2_134 = $set2_134;
# Handle a-z range in set1
if ($expanded_set1_134 =~ /a-z/msx) {
    $expanded_set1_134 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_134 =~ /A-Z/msx) {
    $expanded_set1_134 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_134 =~ /\[:upper:\]/msx) {
    $expanded_set1_134 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_134 =~ /\[:lower:\]/msx) {
    $expanded_set1_134 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_134 =~ /a-z/msx) {
    $expanded_set2_134 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_134 =~ /A-Z/msx) {
    $expanded_set2_134 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_134 =~ /\[:upper:\]/msx) {
    $expanded_set2_134 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_134 =~ /\[:lower:\]/msx) {
    $expanded_set2_134 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_133 = q{};
for my $char ( split //msx, $input_134 ) {
    my $pos_134 = index $expanded_set1_134, $char;
    if ( $pos_134 >= 0 && $pos_134 < length $expanded_set2_134 ) {
        $tr_result_133 .= substr $expanded_set2_134, $pos_134, 1;
    } else {
        $tr_result_133 .= $char;
    }
}
$tr_result_133
}; $_pipeline_result; };
do {
    my $__echo_line = "Lowercase: $tr_result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $xargs_result;
my @xargs_result;
my %xargs_result;
$xargs_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_135 = q{};
    my $output_printed_135;
    my $pipeline_success_135 = 1;
    $output_135 .= '1 2 3' . "\n";
    if ( !($output_135 =~ m{\n\z}msx) ) { $output_135 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_135 = 0; }
    my @xargs_input_135_1 = grep { $_ ne q{} } split /\s+/msx, $output_135;
    my @xargs_output_135_1;
    for my $i (0..scalar @xargs_input_135_1-1) {
        my @xargs_args_135_1;
        for my $j (0..1-1) {
            push @xargs_args_135_1, $xargs_input_135_1[$i + $j];
        }
        my $xargs_line_135_1 = q{};
        $xargs_line_135_1 .= "Number:";
        foreach my $arg (@xargs_args_135_1) {
            $xargs_line_135_1 .= q{ } . $arg;
        }
        push @xargs_output_135_1, $xargs_line_135_1;
    }
    my $xargs_result_135_1 = join "\n", @xargs_output_135_1;
    if ($xargs_result_135_1 ne q{} && !( $xargs_result_135_1 =~ m{\n\z}msx )) { $xargs_result_135_1 .= "\n"; }
    $output_135 = $xargs_result_135_1;

    if ( !$pipeline_success_135 ) { $main_exit_code = 1; }
    $output_135 =~ s/\n+\z//msx;
    $output_135;
}; $_pipeline_result; };
print "Xargs result:\n";
print $xargs_result;
if ( !( ($xargs_result) =~ m{\n\z}msx ) ) { print "\n"; }
if ( -e "file1.txt" ) {
    if ( -d "file1.txt" ) {
        carp "rm: carping: ", "file1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "file1.txt" ) {
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

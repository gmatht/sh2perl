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

$PROGRAM_NAME = '000__04c_text_processing_commands.sh';
my $MAGIC_3 = 3;
my $MAGIC_5 = 5;

print "=== Text Processing Commands ===\n";
my $file_content = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_47 = q{};
    my $output_printed_47;
    my $pipeline_success_47 = 1;
    $output_47 = do { my $cat_chunk = q{}; if ( open my $fh, '<', '000__04c_text_processing_commands.sh' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '000__04c_text_processing_commands.sh' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    if ($CHILD_ERROR != 0) { $pipeline_success_47 = 0; }
    my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_47;
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
    $output_47 = $result;

    if ( !$pipeline_success_47 ) { $main_exit_code = 1; }
    $output_47 =~ s/\n+\z//msx;
    $output_47;
}; $_pipeline_result; };
print "First 5 lines of this file:\n";
print $file_content;
if ( !( ($file_content) =~ m{\n\z}msx ) ) { print "\n"; }
my $grep_result = do { my $grep_result_48;
my @grep_lines_48 = ();
my @grep_filenames_48 = ();
if (-e "000__04c_text_processing_commands.sh") {
    open my $fh, '<', "000__04c_text_processing_commands.sh" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_48, $line;
        push @grep_filenames_48, "000__04c_text_processing_commands.sh";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: 000__04c_text_processing_commands.sh: No such file or directory\n"; }
my @grep_filtered_48 = grep { /echo/msx } @grep_lines_48;
my @grep_numbered_48;
for my $i (0..@grep_lines_48-1) {
    if (scalar grep { $_ eq $grep_lines_48[$i] } @grep_filtered_48) {
        push @grep_numbered_48, sprintf "%d:%s", $i + 1, $grep_lines_48[$i];
    }
}
$grep_result_48 = join "\n", @grep_numbered_48;
$CHILD_ERROR = scalar @grep_filtered_48 > 0 ? 0 : 1;
 $grep_result_48; };
print "Lines containing 'echo':\n";
print $grep_result;
if ( !( ($grep_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $sed_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_49 = q{};
    my $output_printed_49;
    my $pipeline_success_49 = 1;
    $output_49 .= 'Hello World' . "\n";
    if ( !($output_49 =~ m{\n\z}msx) ) { $output_49 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_49 = 0; }
    my @sed_lines_49 = split /\n/msx, $output_49;
    my @sed_result_49;
    foreach my $line (@sed_lines_49) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_49, $line;
    }
    $output_49 = join "\n", @sed_result_49;

    if ( !$pipeline_success_49 ) { $main_exit_code = 1; }
    $output_49 =~ s/\n+\z//msx;
    $output_49;
}; $_pipeline_result; };
do {
    my $output = "Sed result: $sed_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $awk_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_50 = q{};
    my $output_printed_50;
    my $pipeline_success_50 = 1;
    $output_50 .= '1 2 3 4 5' . "\n";
    if ( !($output_50 =~ m{\n\z}msx) ) { $output_50 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_50 = 0; }
    my @lines = split /\n/msx, $output_50;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /\s+/msx, $line;
        push @result, ($fields[0] + $fields[1] . "\n");
    }
    $output_50 = join "", @result;

    if ( !$pipeline_success_50 ) { $main_exit_code = 1; }
    $output_50 =~ s/\n+\z//msx;
    $output_50;
}; $_pipeline_result; };
do {
    my $output = "Awk sum result: $awk_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $sort_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_51 = q{};
    my $output_printed_51;
    my $pipeline_success_51 = 1;
    $output_51 .= "zebra\napple\nbanana";
    if ( !($output_51 =~ m{\n\z}msx) ) { $output_51 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_51 = 0; }
    my @sort_lines_51_1 = split /\n/msx, $output_51;
    my @sort_sorted_51_1 = sort @sort_lines_51_1;
    $output_51 = join "\n", @sort_sorted_51_1;
        if ($output_51 ne q{} && !($output_51 =~ m{\n\z}msx)) {
            $output_51 .= "\n";
        }
    if ( !$pipeline_success_51 ) { $main_exit_code = 1; }
    $output_51 =~ s/\n+\z//msx;
    $output_51;
}; $_pipeline_result; };
print "Sorted words:\n";
print $sort_result;
if ( !( ($sort_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $uniq_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_52 = q{};
    my $output_printed_52;
    my $pipeline_success_52 = 1;
    $output_52 .= "apple\napple\nbanana\nbanana\ncherry";
    if ( !($output_52 =~ m{\n\z}msx) ) { $output_52 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_52 = 0; }
    my @uniq_lines_52_1 = split /\n/msx, $output_52;
    @uniq_lines_52_1 = grep { $_ ne q{} } @uniq_lines_52_1; # Filter out empty lines
    my %uniq_seen_52_1;
    my @uniq_result_52_1;
    foreach my $line (@uniq_lines_52_1) {
    if (!$uniq_seen_52_1{$line}++) { push @uniq_result_52_1, $line; }
    }
    $output_52 = join "\n", @uniq_result_52_1;
        if ($output_52 ne q{} && !($output_52 =~ m{\n\z}msx)) {
            $output_52 .= "\n";
        }
    if ( !$pipeline_success_52 ) { $main_exit_code = 1; }
    $output_52 =~ s/\n+\z//msx;
    $output_52;
}; $_pipeline_result; };
print "Unique words:\n";
print $uniq_result;
if ( !( ($uniq_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $line_count = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_53 = q{};
    my $output_printed_53;
    my $pipeline_success_53 = 1;
    $output_53 .= "line1\nline2\nline3";
    if ( !($output_53 =~ m{\n\z}msx) ) { $output_53 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_53 = 0; }
    $output_53 = do {
            my $_wc_data = $output_53;
            my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
            my $_wc_result = q{};
            $_wc_result .= sprintf q{%d}, $_wc_lines;
            $_wc_result .= "\n";
            $_wc_result;
        };
    if ( !$pipeline_success_53 ) { $main_exit_code = 1; }
    $output_53 =~ s/\n+\z//msx;
    $output_53;
}; $_pipeline_result; };
my $word_count = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_54 = q{};
    my $output_printed_54;
    my $pipeline_success_54 = 1;
    $output_54 .= 'Hello World' . "\n";
    if ( !($output_54 =~ m{\n\z}msx) ) { $output_54 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_54 = 0; }
    $output_54 = do {
            my $_wc_data = $output_54;
            my $_wc_words = scalar split /\s+/msx, $_wc_data;
            my $_wc_result = q{};
            $_wc_result .= sprintf q{%d}, $_wc_words;
            $_wc_result .= "\n";
            $_wc_result;
        };
    if ( !$pipeline_success_54 ) { $main_exit_code = 1; }
    $output_54 =~ s/\n+\z//msx;
    $output_54;
}; $_pipeline_result; };
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
my $head_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    do { my $output_55 = q{};
my $output_printed_55;
do {
    my $seq_output_56 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_56 = split /\n/msx, $seq_output_56;
    my $output_56 = q{};
    my $head_line_count = 0;
    foreach my $line (@seq_lines_56) {
        chomp $line;
        if ($head_line_count < 3) {
    $output_56 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    }
    $output_56;
} };
}; $_pipeline_result; };
do {
    my $output = "First 3 numbers: $head_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $tail_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    do { my $output_57 = q{};
my $output_printed_57;
do {
    my $seq_output_58 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_58 = split /\n/msx, $seq_output_58;
    my $output_58 = q{};
    my @tail_lines = ();
    foreach my $line (@seq_lines_58) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_58 = join "\n", @last_lines;
        if ($output_58 ne q{}) {
            $output_58 .= "\n";
        }
    }
    $output_58;
} };
}; $_pipeline_result; };
do {
    my $output = "Last 3 numbers: $tail_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $cut_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_59 = q{};
    my $output_printed_59;
    my $pipeline_success_59 = 1;
    $output_59 .= 'apple:banana:cherry' . "\n";
    if ( !($output_59 =~ m{\n\z}msx) ) { $output_59 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_59 = 0; }
    my @lines_60 = split /\n/msx, $output_59;
    my @result_60;
    foreach my $line (@lines_60) {
    chomp $line;
    my @fields = split /:/msx, $line;
    if (@fields > 1) {
        push @result_60, $fields[1];
    }
    }
    $output_59 = join "\n", @result_60;
    if ($output_59 ne q{} && !($output_59  =~ m{\n\z}msx)) { $output_59 .= "\n"; }

    if ( !$pipeline_success_59 ) { $main_exit_code = 1; }
    $output_59 =~ s/\n+\z//msx;
    $output_59;
}; $_pipeline_result; };
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
my $paste_result = do {
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
if ( !( ($comm_result) =~ m{\n\z}msx ) ) { print "\n"; }
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
if ( !( ($diff_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $tr_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $input_data = ("HELLO WORLD") . "\n";
    my $set1_62 = 'A-Z';
my $set2_62 = 'a-z';
my $input_62 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_62 = $set1_62;
my $expanded_set2_62 = $set2_62;
# Handle a-z range in set1
if ($expanded_set1_62 =~ /a-z/msx) {
    $expanded_set1_62 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_62 =~ /A-Z/msx) {
    $expanded_set1_62 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_62 =~ /\[:upper:\]/msx) {
    $expanded_set1_62 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_62 =~ /\[:lower:\]/msx) {
    $expanded_set1_62 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_62 =~ /a-z/msx) {
    $expanded_set2_62 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_62 =~ /A-Z/msx) {
    $expanded_set2_62 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_62 =~ /\[:upper:\]/msx) {
    $expanded_set2_62 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_62 =~ /\[:lower:\]/msx) {
    $expanded_set2_62 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_61 = q{};
for my $char ( split //msx, $input_62 ) {
    my $pos_62 = index $expanded_set1_62, $char;
    if ( $pos_62 >= 0 && $pos_62 < length $expanded_set2_62 ) {
        $tr_result_61 .= substr $expanded_set2_62, $pos_62, 1;
    } else {
        $tr_result_61 .= $char;
    }
}
$tr_result_61
}; $_pipeline_result; };
do {
    my $output = "Lowercase: $tr_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $xargs_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_63 = q{};
    my $output_printed_63;
    my $pipeline_success_63 = 1;
    $output_63 .= '1 2 3' . "\n";
    if ( !($output_63 =~ m{\n\z}msx) ) { $output_63 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_63 = 0; }
    my @xargs_input_63_1 = grep { $_ ne q{} } split /\s+/msx, $output_63;
    my @xargs_output_63_1;
    for my $i (0..scalar @xargs_input_63_1-1) {
        my @xargs_args_63_1;
        for my $j (0..1-1) {
            push @xargs_args_63_1, $xargs_input_63_1[$i + $j];
        }
        my $xargs_line_63_1 = q{};
        $xargs_line_63_1 .= "Number:";
        foreach my $arg (@xargs_args_63_1) {
            $xargs_line_63_1 .= q{ } . $arg;
        }
        push @xargs_output_63_1, $xargs_line_63_1;
    }
    my $xargs_result_63_1 = join "\n", @xargs_output_63_1;
    if ($xargs_result_63_1 ne q{} && !( $xargs_result_63_1 =~ m{\n\z}msx )) { $xargs_result_63_1 .= "\n"; }
    $output_63 = $xargs_result_63_1;

    if ( !$pipeline_success_63 ) { $main_exit_code = 1; }
    $output_63 =~ s/\n+\z//msx;
    $output_63;
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
print "=== Text Processing Commands Complete ===\n";

exit $main_exit_code;

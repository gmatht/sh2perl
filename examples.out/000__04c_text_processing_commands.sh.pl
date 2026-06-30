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

my $MAGIC_3 = 3;
my $MAGIC_5 = 5;

print "=== Text Processing Commands ===\n";
my $file_content;
$file_content = do { do {
    my $output_49 = q{};
    my $output_printed_49;
    my $pipeline_success_49 = 1;
    $output_49 = do { open my $fh, '<', '000__04c_text_processing_commands.sh' or die 'cat: ' . '000__04c_text_processing_commands.sh' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; };
    my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_49;
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
    $output_49 = $result;

    if ( !$pipeline_success_49 ) { $main_exit_code = 1; }
    if ($output_49 ne q{} && !($output_49 =~ m{\n\z}msx)) {
        $output_49 .= "\n";
    }
    $output_49;
} };
print "First 5 lines of this file:\n";
print $file_content;
if ( !( $file_content =~ m{\n\z}msx ) ) { print "\n"; }
my $grep_result;
$grep_result = do { my $grep_result_50;
my @grep_lines_50 = ();
my @grep_filenames_50 = ();
if (-e "000__04c_text_processing_commands.sh") {
    open my $fh, '<', "000__04c_text_processing_commands.sh" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_50, $line;
        push @grep_filenames_50, "000__04c_text_processing_commands.sh";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: 000__04c_text_processing_commands.sh: No such file or directory\n"; }
my @grep_filtered_50 = grep { /echo/msx } @grep_lines_50;
my @grep_numbered_50;
for my $i (0..@grep_lines_50-1) {
    if (scalar grep { $_ eq $grep_lines_50[$i] } @grep_filtered_50) {
        push @grep_numbered_50, sprintf "%d:%s", $i + 1, $grep_lines_50[$i];
    }
}
$grep_result_50 = join "\n", @grep_numbered_50;
$CHILD_ERROR = scalar @grep_filtered_50 > 0 ? 0 : 1;
 $grep_result_50; };
print "Lines containing 'echo':\n";
print $grep_result;
if ( !( $grep_result =~ m{\n\z}msx ) ) { print "\n"; }
my $sed_result;
$sed_result = do { do {
    my $output_51 = q{};
    my $output_printed_51;
    my $pipeline_success_51 = 1;
    $output_51 .= 'Hello World' . "\n";
    if ( !($output_51 =~ m{\n\z}msx) ) { $output_51 .= "\n"; }
    $CHILD_ERROR = 0;
    my @sed_lines_51 = split /\n/msx, $output_51;
    my @sed_result_51;
    foreach my $line (@sed_lines_51) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_51, $line;
    }
    $output_51 = join "\n", @sed_result_51;

    if ( !$pipeline_success_51 ) { $main_exit_code = 1; }
    if ($output_51 ne q{} && !($output_51 =~ m{\n\z}msx)) {
        $output_51 .= "\n";
    }
    $output_51;
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
    my $output_52 = q{};
    my $output_printed_52;
    my $pipeline_success_52 = 1;
    $output_52 .= '1 2 3 4 5' . "\n";
    if ( !($output_52 =~ m{\n\z}msx) ) { $output_52 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines = split /\n/msx, $output_52;
    my @result;
    foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        my @fields = split /\s+/msx, $line;
        push @result, ($fields[0] . $fields[1] . "\n");
    }
    $output_52 = join "", @result;

    if ( !$pipeline_success_52 ) { $main_exit_code = 1; }
    if ($output_52 ne q{} && !($output_52 =~ m{\n\z}msx)) {
        $output_52 .= "\n";
    }
    $output_52;
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
    my $output_53 = q{};
    my $output_printed_53;
    my $pipeline_success_53 = 1;
    $output_53 .= "zebra\napple\nbanana";
    if ( !($output_53 =~ m{\n\z}msx) ) { $output_53 .= "\n"; }
    $CHILD_ERROR = 0;
    my @sort_lines_53_1 = split /\n/msx, $output_53;
    my @sort_sorted_53_1 = sort @sort_lines_53_1;
    $output_53 = join "\n", @sort_sorted_53_1;
        if ($output_53 ne q{} && !($output_53 =~ m{\n\z}msx)) {
            $output_53 .= "\n";
        }
    if ( !$pipeline_success_53 ) { $main_exit_code = 1; }
    if ($output_53 ne q{} && !($output_53 =~ m{\n\z}msx)) {
        $output_53 .= "\n";
    }
    $output_53;
} };
print "Sorted words:\n";
print $sort_result;
if ( !( $sort_result =~ m{\n\z}msx ) ) { print "\n"; }
my $uniq_result;
$uniq_result = do { do {
    my $output_54 = q{};
    my $output_printed_54;
    my $pipeline_success_54 = 1;
    $output_54 .= "apple\napple\nbanana\nbanana\ncherry";
    if ( !($output_54 =~ m{\n\z}msx) ) { $output_54 .= "\n"; }
    $CHILD_ERROR = 0;
    my @uniq_lines_54_1 = split /\n/msx, $output_54;
    @uniq_lines_54_1 = grep { $_ ne q{} } @uniq_lines_54_1; # Filter out empty lines
    my %uniq_seen_54_1;
    my @uniq_result_54_1;
    foreach my $line (@uniq_lines_54_1) {
    if (!$uniq_seen_54_1{$line}++) { push @uniq_result_54_1, $line; }
    }
    $output_54 = join "\n", @uniq_result_54_1;
        if ($output_54 ne q{} && !($output_54 =~ m{\n\z}msx)) {
            $output_54 .= "\n";
        }
    if ( !$pipeline_success_54 ) { $main_exit_code = 1; }
    if ($output_54 ne q{} && !($output_54 =~ m{\n\z}msx)) {
        $output_54 .= "\n";
    }
    $output_54;
} };
print "Unique words:\n";
print $uniq_result;
if ( !( $uniq_result =~ m{\n\z}msx ) ) { print "\n"; }
my $word_count;
$word_count = do { do {
    my $output_55 = q{};
    my $output_printed_55;
    my $pipeline_success_55 = 1;
    $output_55 .= 'Hello World' . "\n";
    if ( !($output_55 =~ m{\n\z}msx) ) { $output_55 .= "\n"; }
    $CHILD_ERROR = 0;
    use IPC::Open3;
    my @wc_args_55_1 = ('-w');
    my ($wc_in_55_1, $wc_out_55_1, $wc_err_55_1);
    my $wc_pid_55_1 = open3($wc_in_55_1, $wc_out_55_1, $wc_err_55_1, 'wc', @wc_args_55_1);
    print {$wc_in_55_1} $output_55;
    close $wc_in_55_1 or die "Close failed: $!\n";
    $output_55 = do { local $/ = undef; <$wc_out_55_1> };
    close $wc_out_55_1 or die "Close failed: $!\n";
    waitpid $wc_pid_55_1, 0;
    if ( !$pipeline_success_55 ) { $main_exit_code = 1; }
    if ($output_55 ne q{} && !($output_55 =~ m{\n\z}msx)) {
        $output_55 .= "\n";
    }
    $output_55;
} };
my $line_count;
$line_count = do { do {
    my $output_56 = q{};
    my $output_printed_56;
    my $pipeline_success_56 = 1;
    $output_56 .= "line1\nline2\nline3";
    if ( !($output_56 =~ m{\n\z}msx) ) { $output_56 .= "\n"; }
    $CHILD_ERROR = 0;
    use IPC::Open3;
    my @wc_args_56_1 = ('-l');
    my ($wc_in_56_1, $wc_out_56_1, $wc_err_56_1);
    my $wc_pid_56_1 = open3($wc_in_56_1, $wc_out_56_1, $wc_err_56_1, 'wc', @wc_args_56_1);
    print {$wc_in_56_1} $output_56;
    close $wc_in_56_1 or die "Close failed: $!\n";
    $output_56 = do { local $/ = undef; <$wc_out_56_1> };
    if ($output_56 eq q{}) { $output_56 = "0\n"; }
    close $wc_out_56_1 or die "Close failed: $!\n";
    waitpid $wc_pid_56_1, 0;
    if ( !$pipeline_success_56 ) { $main_exit_code = 1; }
    if ($output_56 ne q{} && !($output_56 =~ m{\n\z}msx)) {
        $output_56 .= "\n";
    }
    $output_56;
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
    my $head_line_count = 0;
    foreach my $line (@seq_lines_58) {
        chomp $line;
        if ($head_line_count < 3) {
    $output_58 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    }
    $output_58 =~ s/\n+\z//msx;
    $output_58;
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
    do { my $output_59 = q{};
my $output_printed_59;
do {
    my $seq_output_60 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_60 = split /\n/msx, $seq_output_60;
    my $output_60 = q{};
    my @tail_lines = ();
    foreach my $line (@seq_lines_60) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_60 = join "\n", @last_lines;
        if ($output_60 ne q{}) {
            $output_60 .= "\n";
        }
    }
    $output_60 =~ s/\n+\z//msx;
    $output_60;
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
    my $output_61 = q{};
    my $output_printed_61;
    my $pipeline_success_61 = 1;
    $output_61 .= 'apple:banana:cherry' . "\n";
    if ( !($output_61 =~ m{\n\z}msx) ) { $output_61 .= "\n"; }
    $CHILD_ERROR = 0;
    my @lines_62 = split /\n/msx, $output_61;
    my @result_62;
    foreach my $line (@lines_62) {
    chomp $line;
    my @fields = split /:/msx, $line;
    if (@fields > 1) {
        push @result_62, $fields[1];
    }
    }
    $output_61 = join "\n", @result_62;
    if (@result_62 && $output_61 !~ /\n$/) { $output_61 .= "\n"; }
    if ( !$pipeline_success_61 ) { $main_exit_code = 1; }
    if ($output_61 ne q{} && !($output_61 =~ m{\n\z}msx)) {
        $output_61 .= "\n";
    }
    $output_61;
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
$paste_result = do {
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
    my $set1_64 = 'A-Z';
my $set2_64 = 'a-z';
my $input_64 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_64 = $set1_64;
my $expanded_set2_64 = $set2_64;
# Handle a-z range in set1
if ($expanded_set1_64 =~ /a-z/msx) {
    $expanded_set1_64 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_64 =~ /A-Z/msx) {
    $expanded_set1_64 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle a-z range in set2
if ($expanded_set2_64 =~ /a-z/msx) {
    $expanded_set2_64 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_64 =~ /A-Z/msx) {
    $expanded_set2_64 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
my $tr_result_63 = q{};
for my $char ( split //msx, $input_64 ) {
    my $pos_64 = index $expanded_set1_64, $char;
    if ( $pos_64 >= 0 && $pos_64 < length $expanded_set2_64 ) {
        $tr_result_63 .= substr $expanded_set2_64, $pos_64, 1;
    } else {
        $tr_result_63 .= $char;
    }
}
$tr_result_63
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
    my $output_65 = q{};
    my $output_printed_65;
    my $pipeline_success_65 = 1;
    $output_65 .= '1 2 3' . "\n";
    if ( !($output_65 =~ m{\n\z}msx) ) { $output_65 .= "\n"; }
    $CHILD_ERROR = 0;
    my @xargs_input_65_1 = split /\n/msx, $output_65;
    my @xargs_output_65_1;
    for my $i (0..scalar @xargs_input_65_1-1) {
        my @xargs_args_65_1;
        for my $j (0..1-1) {
            push @xargs_args_65_1, $xargs_input_65_1[$i + $j];
        }
        my ($in_65_1, $out_65_1, $err_65_1);
        my $pid_65_1 = open3($in_65_1, $out_65_1, $err_65_1, '1', 'echo', 'Number:', @xargs_args_65_1);
        close $in_65_1 or croak 'Close failed: $OS_ERROR';
        my $xargs_result_65_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_65_1> };
        close $out_65_1 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_65_1, 0;
        chomp $xargs_result_65_1;
        push @xargs_output_65_1, $xargs_result_65_1;
    }
    my $xargs_result_65_1 = join "\n", @xargs_output_65_1;
    if ($xargs_result_65_1 ne q{} && !( $xargs_result_65_1 =~ m{\n\z}msx )) { $xargs_result_65_1 .= "\n"; }
    $output_65 = $xargs_result_65_1;

    if ( !$pipeline_success_65 ) { $main_exit_code = 1; }
    if ($output_65 ne q{} && !($output_65 =~ m{\n\z}msx)) {
        $output_65 .= "\n";
    }
    $output_65;
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
print "=== Text Processing Commands Complete ===\n";

exit $main_exit_code;

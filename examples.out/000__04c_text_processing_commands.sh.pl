#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

my $MAGIC_5 = 5;
my $MAGIC_3 = 3;

print "=== Text Processing Commands ===\n";
my $file_content = do {
    my $output_51;
    my $pipeline_success_51 = 1;
    do { my $cat_cmd = 'cat 000__04c_text_processing_commands.sh'; $output_51 = qx{$cat_cmd}; };
    my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_51;
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
    $output_51 = $result;

    if ( !$pipeline_success_51 ) { $main_exit_code = 1; }
    $output_51 =~ s/\n+\z//msx;
    $output_51;
};
print "First 5 lines of this file:\n";
print $file_content;
if ( !( $file_content =~ m{\n\z}msx ) ) { print "\n"; }
my $grep_result = do { my $grep_result_52;
my @grep_lines_52 = ();
my @grep_filenames_52 = ();
if (-e "000__04c_text_processing_commands.sh") {
    open my $fh, '<', "000__04c_text_processing_commands.sh" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_52, $line;
        push @grep_filenames_52, "000__04c_text_processing_commands.sh";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: 000__04c_text_processing_commands.sh: No such file or directory\n"; }
my @grep_filtered_52 = grep { /echo/msx } @grep_lines_52;
my @grep_numbered_52;
for my $i (0..@grep_lines_52-1) {
    if (scalar grep { $_ eq $grep_lines_52[$i] } @grep_filtered_52) {
        push @grep_numbered_52, sprintf "%d:%s", $i + 1, $grep_lines_52[$i];
    }
}
$grep_result_52 = join "\n", @grep_numbered_52;
$CHILD_ERROR = scalar @grep_filtered_52 > 0 ? 0 : 1;
 $grep_result_52; };
print "Lines containing 'echo':\n";
print $grep_result;
if ( !( $grep_result =~ m{\n\z}msx ) ) { print "\n"; }
my $sed_result = do {
    my $output_53;
    my $pipeline_success_53 = 1;
    $output_53 .= "Hello World\n";
    if ( !($output_53 =~ m{\n\z}msx) ) { $output_53 .= "\n"; }
    my @sed_lines_53 = split /\n/msx, $output_53;
    my @sed_result_53;
    foreach my $line (@sed_lines_53) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_53, $line;
    }
    $output_53 = join "\n", @sed_result_53;

    if ( !$pipeline_success_53 ) { $main_exit_code = 1; }
    $output_53 =~ s/\n+\z//msx;
    $output_53;
};
do {
    my $output = "Sed result: $sed_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $awk_result = do {
    my $output_54;
    my $pipeline_success_54 = 1;
    $output_54 .= "1 2 3 4 5\n";
    if ( !($output_54 =~ m{\n\z}msx) ) { $output_54 .= "\n"; }
    my @lines = split /\n/msx, $output_54;
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
    $output_54 = join "\n", @result;

    if ( !$pipeline_success_54 ) { $main_exit_code = 1; }
    $output_54 =~ s/\n+\z//msx;
    $output_54;
};
do {
    my $output = "Awk sum result: $awk_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $sort_result = do {
    my $output_55;
    my $pipeline_success_55 = 1;
    $output_55 .= "zebra\napple\nbanana";
    if ( !($output_55 =~ m{\n\z}msx) ) { $output_55 .= "\n"; }
    my @sort_lines_55_1 = split /\n/msx, $output_55;
    my @sort_sorted_55_1 = sort @sort_lines_55_1;
    $output_55 = join "\n", @sort_sorted_55_1;
        if ($output_55 ne q{} && !($output_55 =~ m{\n\z}msx)) {
            $output_55 .= "\n";
        }
    if ( !$pipeline_success_55 ) { $main_exit_code = 1; }
    $output_55 =~ s/\n+\z//msx;
    $output_55;
};
print "Sorted words:\n";
print $sort_result;
if ( !( $sort_result =~ m{\n\z}msx ) ) { print "\n"; }
my $uniq_result = do {
    my $output_56;
    my $pipeline_success_56 = 1;
    $output_56 .= "apple\napple\nbanana\nbanana\ncherry";
    if ( !($output_56 =~ m{\n\z}msx) ) { $output_56 .= "\n"; }
    my @uniq_lines_56_1 = split /\n/msx, $output_56;
    @uniq_lines_56_1 = grep { $_ ne q{} } @uniq_lines_56_1; # Filter out empty lines
    my %uniq_seen_56_1;
    my @uniq_result_56_1;
    foreach my $line (@uniq_lines_56_1) {
    if (!$uniq_seen_56_1{$line}++) { push @uniq_result_56_1, $line; }
    }
    $output_56 = join "\n", @uniq_result_56_1;
        if ($output_56 ne q{} && !($output_56 =~ m{\n\z}msx)) {
            $output_56 .= "\n";
        }
    if ( !$pipeline_success_56 ) { $main_exit_code = 1; }
    $output_56 =~ s/\n+\z//msx;
    $output_56;
};
print "Unique words:\n";
print $uniq_result;
if ( !( $uniq_result =~ m{\n\z}msx ) ) { print "\n"; }
my $line_count = do {
    my $output_57;
    my $pipeline_success_57 = 1;
    $output_57 .= "line1\nline2\nline3";
    if ( !($output_57 =~ m{\n\z}msx) ) { $output_57 .= "\n"; }
    use IPC::Open3;
    my @wc_args_57_1 = ("-l");
    my ($wc_in_57_1, $wc_out_57_1, $wc_err_57_1);
    my $wc_pid_57_1 = open3($wc_in_57_1, $wc_out_57_1, $wc_err_57_1, 'wc', @wc_args_57_1);
    print {$wc_in_57_1} $output_57;
    close $wc_in_57_1 or die "Close failed: $!\n";
    $output_57 = do { local $/ = undef; <$wc_out_57_1> };
    close $wc_out_57_1 or die "Close failed: $!\n";
    waitpid $wc_pid_57_1, 0;
    if ( !$pipeline_success_57 ) { $main_exit_code = 1; }
    $output_57 =~ s/\n+\z//msx;
    $output_57;
};
my $word_count = do {
    my $output_58;
    my $pipeline_success_58 = 1;
    $output_58 .= "Hello World\n";
    if ( !($output_58 =~ m{\n\z}msx) ) { $output_58 .= "\n"; }
    use IPC::Open3;
    my @wc_args_58_1 = ("-w");
    my ($wc_in_58_1, $wc_out_58_1, $wc_err_58_1);
    my $wc_pid_58_1 = open3($wc_in_58_1, $wc_out_58_1, $wc_err_58_1, 'wc', @wc_args_58_1);
    print {$wc_in_58_1} $output_58;
    close $wc_in_58_1 or die "Close failed: $!\n";
    $output_58 = do { local $/ = undef; <$wc_out_58_1> };
    close $wc_out_58_1 or die "Close failed: $!\n";
    waitpid $wc_pid_58_1, 0;
    if ( !$pipeline_success_58 ) { $main_exit_code = 1; }
    $output_58 =~ s/\n+\z//msx;
    $output_58;
};
do {
    my $output = "Word count: $word_count";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
do {
    my $output = "Line count: $line_count";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $head_result = do {
    my $seq_output_60 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_60 = split /\n/msx, $seq_output_60;
    my $output_60 = q{};
    my $head_line_count = 0;
    foreach my $line (@seq_lines_60) {
        chomp $line;
        if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_60 .= "\n"; }
    $output_60 .= $line;
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    }
    $output_60 =~ s/\n+\z//msx;
    $output_60;
};
do {
    my $output = "First 3 numbers: $head_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $tail_result = do {
    my $seq_output_62 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_62 = split /\n/msx, $seq_output_62;
    my $output_62 = q{};
    my @tail_lines = ();
    foreach my $line (@seq_lines_62) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_62 = join "\n", @last_lines;
        if ($output_62 ne q{}) {
            $output_62 .= "\n";
        }
    }
    $output_62 =~ s/\n+\z//msx;
    $output_62;
};
do {
    my $output = "Last 3 numbers: $tail_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $cut_result = do {
    my $output_63;
    my $pipeline_success_63 = 1;
    $output_63 .= "apple:banana:cherry\n";
    if ( !($output_63 =~ m{\n\z}msx) ) { $output_63 .= "\n"; }
    my @lines_64 = split /\n/msx, $output_63;
    my @result_64;
    foreach my $line (@lines_64) {
    chomp $line;
    my @fields = split /\:/msx, $line;
    if (@fields > 1) {
    push @result_64, $fields[1];
    }
    }
    $output_63 = join "\n", @result_64;

    if ( !$pipeline_success_63 ) { $main_exit_code = 1; }
    $output_63 =~ s/\n+\z//msx;
    $output_63;
};
do {
    my $output = "Second field: $cut_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
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
    carp "rm: carping: ", "temp1.txt", ": No such file or directory\n";
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
    carp "rm: carping: ", "temp2.txt", ": No such file or directory\n";
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
my $tr_result = do {
    my $_chomp_result = do {
    my $input_data = ("HELLO WORLD");
    my $set1_66 = 'A-Z';
my $set2_66 = 'a-z';
my $input_66 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_66 = $set1_66;
my $expanded_set2_66 = $set2_66;
# Handle a-z range in set1
if ($expanded_set1_66 =~ /a-z/msx) {
    $expanded_set1_66 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_66 =~ /A-Z/msx) {
    $expanded_set1_66 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle a-z range in set2
if ($expanded_set2_66 =~ /a-z/msx) {
    $expanded_set2_66 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_66 =~ /A-Z/msx) {
    $expanded_set2_66 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
my $tr_result_65 = q{};
for my $char ( split //msx, $input_66 ) {
    my $pos_66 = index $expanded_set1_66, $char;
    if ( $pos_66 >= 0 && $pos_66 < length $expanded_set2_66 ) {
        $tr_result_65 .= substr $expanded_set2_66, $pos_66, 1;
    } else {
        $tr_result_65 .= $char;
    }
}
$tr_result_65
};
    chomp $_chomp_result;
    $_chomp_result;
};
do {
    my $output = "Lowercase: $tr_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $xargs_result = do {
    my $output_67;
    my $pipeline_success_67 = 1;
    $output_67 .= "1 2 3\n";
    if ( !($output_67 =~ m{\n\z}msx) ) { $output_67 .= "\n"; }
    my @xargs_input_67_1 = split /\s+/msx, $output_67;
    my @xargs_output_67_1;
    for my $i (0..scalar @xargs_input_67_1-1) {
        my @xargs_args_67_1;
        for my $j (0..1-1) {
            push @xargs_args_67_1, $xargs_input_67_1[$i + $j];
        }
        my $xargs_line_67_1 = q{};
        $xargs_line_67_1 .= "Number:";
        foreach my $arg (@xargs_args_67_1) {
            $xargs_line_67_1 .= q{ } . $arg;
        }
        push @xargs_output_67_1, $xargs_line_67_1;
    }
    my $xargs_result_67_1 = join "\n", @xargs_output_67_1;
    $output_67 = $xargs_result_67_1;

    if ( !$pipeline_success_67 ) { $main_exit_code = 1; }
    $output_67 =~ s/\n+\z//msx;
    $output_67;
};
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
    carp "rm: carping: ", "file1.txt", ": No such file or directory\n";
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
    carp "rm: carping: ", "file2.txt", ": No such file or directory\n";
}
print "=== Text Processing Commands Complete ===\n";

exit $main_exit_code;

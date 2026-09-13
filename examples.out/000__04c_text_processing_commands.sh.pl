#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;
$0 = '000__04c_text_processing_commands.sh';
print "=== Text Processing Commands ===\n";
my $file_content = do { open(my $__fh, '-|', 'bash', '-c', 'cat 000__04c_text_processing_commands.sh | head -5') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "First 5 lines of this file:\n";
print($file_content, "\n");
my $grep_result = do { my $__cs = do { my $grep_result_49;
my @grep_lines_49 = ();
my @grep_filenames_49 = ();
if (-e "000__04c_text_processing_commands.sh") {
    open my $fh, '<', "000__04c_text_processing_commands.sh" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_49, $line;
        push @grep_filenames_49, "000__04c_text_processing_commands.sh";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: 000__04c_text_processing_commands.sh: No such file or directory\n"; }
my @grep_filtered_49 = grep { /echo/ } @grep_lines_49;
my @grep_numbered_49;
for my $i (0..@grep_lines_49-1) {
    if (scalar grep { $_ eq $grep_lines_49[$i] } @grep_filtered_49) {
        push @grep_numbered_49, sprintf "%d:%s", $i + 1, $grep_lines_49[$i];
    }
}
$grep_result_49 = join "\n", @grep_numbered_49;
$CHILD_ERROR = scalar @grep_filtered_49 > 0 ? 0 : 1;
 $grep_result_49; }; chomp $__cs; $__cs; };
print "Lines containing 'echo':\n";
print($grep_result, "\n");
my $sed_result = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'Hello World' | sed s/World/Universe/}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Sed result: ${sed_result}\n";
my $awk_result = do { open(my $__fh, '-|', 'bash', '-c', q(echo '1 2 3 4 5' | awk '{print $1 + $2}')) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Awk sum result: ${awk_result}\n";
my $sort_result = do { open(my $__fh, '-|', 'bash', '-c', 'echo -e "zebra\\\\napple\\\\nbanana" | sort') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Sorted words:\n";
print($sort_result, "\n");
my $uniq_result = do { open(my $__fh, '-|', 'bash', '-c', 'echo -e "apple\\\\napple\\\\nbanana\\\\nbanana\\\\ncherry" | uniq') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Unique words:\n";
print($uniq_result, "\n");
my $word_count = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'Hello World' | wc -w}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
my $line_count = do { open(my $__fh, '-|', 'bash', '-c', 'echo -e "line1\\\\nline2\\\\nline3" | wc -l') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Word count: ${word_count}\n";
print "Line count: ${line_count}\n";
my $head_result = do { open(my $__fh, '-|', 'bash', '-c', 'seq 1 10 | head -3') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "First 3 numbers: ${head_result}\n";
my $tail_result = do { open(my $__fh, '-|', 'bash', '-c', 'seq 1 10 | tail -3') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Last 3 numbers: ${tail_result}\n";
my $cut_result = do { open(my $__fh, '-|', 'bash', '-c', 'echo apple:banana:cherry | cut -d : -f 2') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Second field: ${cut_result}\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp1.txt'
      or die "Cannot access file: $OS_ERROR\n";
    print "1\n2\n3\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp2.txt'
      or die "Cannot access file: $OS_ERROR\n";
    print "a\nb\nc\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $paste_result = do { my $__cs = do {
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
; chomp $__cs; $__cs; };
print "Pasted columns:\n";
print($paste_result, "\n");
unlink('temp1.txt');
unlink('temp2.txt');
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file1.txt'
      or die "Cannot access file: $OS_ERROR\n";
    print "apple\nbanana\ncherry\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file2.txt'
      or die "Cannot access file: $OS_ERROR\n";
    print "banana\ncherry\ndate\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $comm_result = do { my $__cs = do { my @file1_lines;
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
$comm_output }; chomp $__cs; $__cs; };
print "Common lines:\n";
print($comm_result, "\n");
my $diff_result = do { my $diff_output = do { open(my $__fh, '-|', 'bash', '-c', q{'diff' 'file1.txt' 'file2.txt'}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
$diff_output;
 };
print "File differences:\n";
print($diff_result, "\n");
my $tr_result = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'HELLO WORLD' | tr A-Z a-z}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Lowercase: ${tr_result}\n";
my $xargs_result = do { open(my $__fh, '-|', 'bash', '-c', q{echo '1 2 3' | xargs -n 1 echo Number:}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Xargs result:\n";
print($xargs_result, "\n");
unlink('file1.txt');
unlink('file2.txt');
print "=== Text Processing Commands Complete ===\n";

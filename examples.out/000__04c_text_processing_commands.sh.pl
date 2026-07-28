#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04c_text_processing_commands.sh';
say "=== Text Processing Commands ===";
my $file_content = do { chomp(my $result_48 = qx{cat 000__04c_text_processing_commands.sh | head -5}); $result_48; };
say "First 5 lines of this file:";
say $file_content;
my $grep_result = do { my $grep_result_49;
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
my @grep_filtered_49 = grep { /echo/msx } @grep_lines_49;
my @grep_numbered_49;
for my $i (0..@grep_lines_49-1) {
    if (scalar grep { $_ eq $grep_lines_49[$i] } @grep_filtered_49) {
        push @grep_numbered_49, sprintf "%d:%s", $i + 1, $grep_lines_49[$i];
    }
}
$grep_result_49 = join "\n", @grep_numbered_49;
$CHILD_ERROR = scalar @grep_filtered_49 > 0 ? 0 : 1;
 $grep_result_49; };
say "Lines containing 'echo':";
say $grep_result;
my $sed_result = do { chomp(my $result_50 = qx{echo 'Hello World' | sed s/World/Universe/}); $result_50; };
say "Sed result: $sed_result";
my $awk_result = do { chomp(my $result_51 = qx(echo '1 2 3 4 5' | awk '{print $1 + $2}')); $result_51; };
say "Awk sum result: $awk_result";
my $sort_result = do { chomp(my $result_52 = qx{echo -e "zebra\\napple\\nbanana" | sort}); $result_52; };
say "Sorted words:";
say $sort_result;
my $uniq_result = do { chomp(my $result_53 = qx{echo -e "apple\\napple\\nbanana\\nbanana\\ncherry" | uniq}); $result_53; };
say "Unique words:";
say $uniq_result;
my $word_count = do { chomp(my $result_54 = qx{echo 'Hello World' | wc -w}); $result_54; };
my $line_count = do { chomp(my $result_55 = qx{echo -e "line1\\nline2\\nline3" | wc -l}); $result_55; };
say "Word count: $word_count";
say "Line count: $line_count";
my $head_result = do { chomp(my $result_56 = qx{seq 1 10 | head -3}); $result_56; };
say "First 3 numbers: $head_result";
my $tail_result = do { chomp(my $result_57 = qx{seq 1 10 | tail -3}); $result_57; };
say "Last 3 numbers: $tail_result";
my $cut_result = do { chomp(my $result_58 = qx{echo apple:banana:cherry | cut -d : -f 2}); $result_58; };
say "Second field: $cut_result";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp1.txt'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    say "1\n2\n3";
    };
    print $tmp;
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
    my $tmp = do {
    say "a\nb\nc";
    };
    print $tmp;
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
say "Pasted columns:";
say $paste_result;
unlink('temp1.txt');
unlink('temp2.txt');
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file1.txt'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    say "apple\nbanana\ncherry";
    };
    print $tmp;
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
    my $tmp = do {
    say "banana\ncherry\ndate";
    };
    print $tmp;
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
say "Common lines:";
say $comm_result;
my $diff_result = do { my $diff_output = qx{'diff' 'file1.txt' 'file2.txt'};
chomp $diff_output;
$diff_output;
 };
say "File differences:";
say $diff_result;
my $tr_result = do { chomp(my $result_59 = qx{echo 'HELLO WORLD' | tr A-Z a-z}); $result_59; };
say "Lowercase: $tr_result";
my $xargs_result = do { chomp(my $result_60 = qx{echo '1 2 3' | xargs -n 1 echo Number:}); $result_60; };
say "Xargs result:";
say $xargs_result;
unlink('file1.txt');
unlink('file2.txt');
say "=== Text Processing Commands Complete ===";

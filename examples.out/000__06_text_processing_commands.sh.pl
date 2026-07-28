#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__06_text_processing_commands.sh';
say "=== Text Processing Commands ===";
my $file_content = do { chomp(my $result_113 = qx{cat src/main.rs | head -5}); $result_113; };
say "First 5 lines of main.rs:";
say $file_content;
my $grep_result = do { my $grep_result_114;
my @grep_lines_114 = ();
my @grep_filenames_114 = ();
if (-e "src/main.rs") {
    open my $fh, '<', "src/main.rs" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_114, $line;
        push @grep_filenames_114, "src/main.rs";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: src/main.rs: No such file or directory\n"; }
my @grep_filtered_114 = grep { /fn/msx } @grep_lines_114;
my @grep_numbered_114;
for my $i (0..@grep_lines_114-1) {
    if (scalar grep { $_ eq $grep_lines_114[$i] } @grep_filtered_114) {
        push @grep_numbered_114, sprintf "%d:%s", $i + 1, $grep_lines_114[$i];
    }
}
$grep_result_114 = join "\n", @grep_numbered_114;
$CHILD_ERROR = scalar @grep_filtered_114 > 0 ? 0 : 1;
 $grep_result_114; };
say "Lines containing 'fn':";
say $grep_result;
my $sed_result = do { chomp(my $result_115 = qx{echo 'Hello World' | sed s/World/Universe/}); $result_115; };
say "Sed result: $sed_result";
my $awk_result = do { chomp(my $result_116 = qx(echo '1 2 3 4 5' | awk '{print $1 + $2}')); $result_116; };
say "Awk sum result: $awk_result";
my $sort_result = do { chomp(my $result_117 = qx{echo -e "zebra\\napple\\nbanana" | sort}); $result_117; };
say "Sorted words:";
say $sort_result;
my $uniq_result = do { chomp(my $result_118 = qx{echo -e "apple\\napple\\nbanana\\nbanana\\ncherry" | uniq}); $result_118; };
say "Unique words:";
say $uniq_result;
my $word_count = do { chomp(my $result_119 = qx{echo 'Hello World' | wc -w}); $result_119; };
my $line_count = do { chomp(my $result_120 = qx{echo -e "line1\\nline2\\nline3" | wc -l}); $result_120; };
say "Word count: $word_count";
say "Line count: $line_count";
my $head_result = do { chomp(my $result_121 = qx{seq 1 10 | head -3}); $result_121; };
say "First 3 numbers: $head_result";
my $tail_result = do { chomp(my $result_122 = qx{seq 1 10 | tail -3}); $result_122; };
say "Last 3 numbers: $tail_result";
my $cut_result = do { chomp(my $result_123 = qx{echo apple:banana:cherry | cut -d : -f 2}); $result_123; };
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
my $paste_result = do { chomp(my $result_124 = qx{paste temp1.txt temp2.txt | sed "s/\\t/ /g"}); $result_124; };
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
my $tr_result = do { chomp(my $result_125 = qx{echo 'HELLO WORLD' | tr A-Z a-z}); $result_125; };
say "Lowercase: $tr_result";
my $xargs_result = do { chomp(my $result_126 = qx{echo '1 2 3' | xargs -n 1 echo Number:}); $result_126; };
say "Xargs result:";
say $xargs_result;
unlink('file1.txt');
unlink('file2.txt');

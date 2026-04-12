Running shell script: examples/000__backticks.sh
DEBUG: Starting parse_array_elements
DEBUG: Skipping opening parenthesis
DEBUG: Loop 1: token = Some(BacktickString), current_element = ''
DEBUG: Found other token: '`ls -1 examples/*.sh 2>/dev/null`'
DEBUG: Loop 2: token = Some(ParenClose), current_element = '`ls -1 examples/*.sh 2>/dev/null`'
DEBUG: Found closing parenthesis, adding current_element: '`ls -1 examples/*.sh 2>/dev/null`'
DEBUG: Final elements: ["`ls -1 examples/*.sh 2>/dev/null`"]
Generated Perl code:
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;

my $main_exit_code = 0;

my $MAGIC_5 = 5;
my $MAGIC_3 = 3;

print "=== Basic Backtick Usage ===\n";
print "Current date: " . (do { use POSIX qw(strftime); strftime('%Y', localtime); }) . "\n";
print "Current directory: " . (do { use Cwd; getcwd(); }) . "\n";
my $current_date = do { use POSIX qw(strftime); strftime('%Y%m', localtime); };
my $current_dir = do { use Cwd; getcwd(); };
print "Stored date: $current_date\n";
print "Stored directory: $current_dir\n";
print "=== File and Directory Operations ===\n";
my $file_list = do {
my @ls_files_0 = ();
if (opendir my $dh, q{.}) {
    while (my $file = readdir $dh) {
        push @ls_files_0, $file;
    }
    closedir $dh;
    @ls_files_0 = sort { $a cmp $b } @ls_files_0;
}
join "\n", @ls_files_0;
};
print "File listing:\n";
print $file_list, "\n";
my $found_files = do {
    my @results;
    my $start_path = q{.};
    use File::Find;
    find(sub {
        my $file = $File::Find::name;
        my $filename = $_;
        if (!-f $file) {
            return;
        }
        if ($filename !~ /.*[.]sh$/msx) {
            return;
        }
        push @results, $file;
    }, $start_path);
    join "\n", @results;
};
print "Found shell scripts:\n";
print $found_files, "\n";
my $script_name = do { my $basename_path; my $basename_suffix; $basename_path = $PROGRAM_NAME; $basename_suffix = q{}; if ($basename_suffix ne q{}) { $basename_path =~ s/\Q$basename_suffix\E$//msx; } $basename_path =~ s/.*\///msx; $basename_path; };
my $script_dir = do { my $path; $path = $PROGRAM_NAME; if ($path =~ /\//msx) { $path =~ s/\/[^\/]*$//msx; if ($path eq q{}) { $path = q{.}; } } else { $path = q{.}; } $path; };
print "Script name: $script_name\n";
print "Script directory: $script_dir\n";
print "=== Text Processing Commands ===\n";
my $file_content = do {
    my $output_1;
    my $pipeline_success_1 = 1;
        $output_1 = q{};
    if (open my $fh, '<', '/etc/passwd') {
    while (my $line = <$fh>) {
    $output_1 .= $line;
    }
    close $fh or croak "Close failed: $OS_ERROR";
    # Ensure content ends with newline to prevent line concatenation
        if (!($output_1 =~ /\n$/msx)) {
            $output_1 .= "\n";
        }
    } else {
    carp "cat: /etc/passwd: No such file or directory";
    $output_1 = q{};
    }
    my $num_lines = 5;
    my $head_line_count = 0;
    my $result = q{};
    my $input = $output_1;
    my $pos = 0;
    while ($pos < length $input && $head_line_count < $num_lines) {
        my $line_end = index $input, "\n", $pos;
        if ($line_end == -1) {
            $line_end = length $input;
        }
        my $line = substr $input, $pos, $line_end - $pos;
        $result .= $line . "\n";
        $pos = $line_end + 1;
        ++$head_line_count;
    }
    $output_1 = $result;
    $output_1;
    if (!$pipeline_success_1) { $main_exit_code = 1; }
        chomp $output_1;
    $output_1 =~ s/\n/ /gsxm;
};
print "First 5 lines of /etc/passwd:\n";
print $file_content, "\n";
my $grep_result = do { my @grep_lines_2; my $fh_2; if (-f '/etc/passwd') { open $fh_2, '<', '/etc/passwd' or croak "Cannot open file: $OS_ERROR"; @grep_lines_2 = <$fh_2>; close $fh_2 or croak "Close failed: $OS_ERROR"; chomp @grep_lines_2; @grep_lines_2 = grep { /bash/msx } @grep_lines_2; } join "\n", @grep_lines_2; };
print "Lines containing 'bash':\n";
print $grep_result, "\n";
my $sed_result = do {
    my $output_3;
    my $pipeline_success_3 = 1;
        $output_3 .= "Hello World\n";
    my @sed_lines_3 = split /\n/msx, $output_3;
    my @sed_result_3;
    foreach my $line (@sed_lines_3) {
    chomp $line;
    $line =~ s/World/Universe/gmsx;
    push @sed_result_3, $line;
    }
    $output_3 = join "\n", @sed_result_3;
    $output_3;
    if (!$pipeline_success_3) { $main_exit_code = 1; }
        chomp $output_3;
    $output_3 =~ s/\n/ /gsxm;
};
print "Sed result: $sed_result\n";
my $awk_result = do {
    my $output_4;
    my $pipeline_success_4 = 1;
        $output_4 .= "1 2 3 4 5\n";
    my @lines = split /\n/msx, $output_4;
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
    $output_4 = join "\n", @result;
    $output_4;
    if (!$pipeline_success_4) { $main_exit_code = 1; }
        chomp $output_4;
    $output_4 =~ s/\n/ /gsxm;
};
print "Awk sum result: $awk_result\n";
my $sort_result = do {
    my $output_5;
    my $pipeline_success_5 = 1;
        $output_5 .= "zebra\napple\nbanana";
    my @sort_lines_5_1 = split /\n/msx, $output_5;
    my @sort_sorted_5_1 = sort @sort_lines_5_1;
    $output_5 = join "\n", @sort_sorted_5_1;
        if (!($output_5 =~ /\n$/msx)) {
            $output_5 .= "\n";
        }
    $output_5;
    if (!$pipeline_success_5) { $main_exit_code = 1; }
        chomp $output_5;
    $output_5 =~ s/\n/ /gsxm;
};
print "Sorted words:\n";
print $sort_result, "\n";
my $uniq_result = do {
    my $output_6;
    my $pipeline_success_6 = 1;
        $output_6 .= "apple\napple\nbanana\nbanana\ncherry";
    my @uniq_lines_6_1 = split /\n/msx, $output_6;
    @uniq_lines_6_1 = grep { $_ ne q{} } @uniq_lines_6_1; # Filter out empty lines
    my %uniq_seen_6_1;
    my @uniq_result_6_1;
    foreach my $line (@uniq_lines_6_1) {
    if (!$uniq_seen_6_1{$line}++) { push @uniq_result_6_1, $line; }
    }
    $output_6 = join "\n", @uniq_result_6_1;
        if (!($output_6 =~ /\n$/msx)) {
            $output_6 .= "\n";
        }
    $output_6;
    if (!$pipeline_success_6) { $main_exit_code = 1; }
        chomp $output_6;
    $output_6 =~ s/\n/ /gsxm;
};
print "Unique words:\n";
print $uniq_result, "\n";
my $word_count = do {
    my $output_7;
    my $pipeline_success_7 = 1;
        $output_7 .= "Hello World\n";
    my @wc_lines_7_1 = split /\n/msx, $output_7;
    my $wc_word_count_7_1 = 0;
    foreach my $line (@wc_lines_7_1) {
        my @wc_words_7_1 = split /\s+/msx, $line;
        $wc_word_count_7_1 += scalar @wc_words_7_1;
    }
    $output_7 = q{};
    $output_7 .= "$wc_word_count_7_1 ";
    $output_7 =~ s/\s+$//msx;
    $output_7;
    if (!$pipeline_success_7) { $main_exit_code = 1; }
        chomp $output_7;
    $output_7 =~ s/\n/ /gsxm;
};
my $line_count = do {
    my $output_8;
    my $pipeline_success_8 = 1;
        $output_8 .= "line1\nline2\nline3";
    my @wc_lines_8_1 = split /\n/msx, $output_8;
    my $wc_line_count_8_1 = scalar @wc_lines_8_1;
    $output_8 = q{};
    $output_8 .= "$wc_line_count_8_1 ";
    $output_8 =~ s/\s+$//msx;
    $output_8;
    if (!$pipeline_success_8) { $main_exit_code = 1; }
        chomp $output_8;
    $output_8 =~ s/\n/ /gsxm;
};
print "Word count: $word_count\n";
print "Line count: $line_count\n";
my $head_result = do {
    my $seq_output_10 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_10 = split /\n/msx, $seq_output_10;
    my $output_10 = q{};
    my $head_line_count = 0;
    foreach my $line (@seq_lines_10) {
        chomp $line;
        if ($head_line_count < 3) {
    $output_10 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
}
    }
    $output_10;
    chomp $output_10;
    my @temp_lines_10 = split /\n/msx, $output_10;
    $output_10 = join q{ }, @temp_lines_10;
}
;
print "First 3 numbers: $head_result\n";
my $tail_result = do {
    my $seq_output_12 = do {
    my $result = q{};
    for my $i (1..10) {
        $result .= "$i\n";
    }
    $result;
};
    my @seq_lines_12 = split /\n/msx, $seq_output_12;
    my $output_12 = q{};
    my @tail_lines = ();
    foreach my $line (@seq_lines_12) {
        chomp $line;
        # tail -3: collecting all lines first (pipeline limitation)
        push @tail_lines, $line;
        $line = q{}; # Clear line to prevent printing
    }
    if (@tail_lines > 0) {
        my @last_lines = @tail_lines[-3..-1];
        $output_12 = join "\n", @last_lines;
        if ($output_12 ne q{}) {
            $output_12 .= "\n";
        }
    }
    $output_12;
    chomp $output_12;
    my @temp_lines_12 = split /\n/msx, $output_12;
    $output_12 = join q{ }, @temp_lines_12;
}
;
print "Last 3 numbers: $tail_result\n";
my $cut_result = do {
    my $output_13;
    my $pipeline_success_13 = 1;
        $output_13 .= "apple:banana:cherry\n";
    my @lines_14 = split /\n/msx, $output_13;
    my @result_14;
    foreach my $line (@lines_14) {
    chomp $line;
    my @fields = split /:/msx, $line;
    if (@fields > 0) {
    push @result_14, $fields[0];
    }
    }
    $output_13 = join "\n", @result_14;
    $output_13;
    if (!$pipeline_success_13) { $main_exit_code = 1; }
        chomp $output_13;
    $output_13 =~ s/\n/ /gsxm;
};
print "Second field: $cut_result\n";
my $paste_result =  my ($in_15, $out_15, $err_15); my $pid_15 = open3($in_15, $out_15, $err_15, 'paste'); close $in_15 or croak 'Close failed: $!'; my $result_15 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_15> }; close $out_15 or croak 'Close failed: $!'; waitpid $pid_15, 0; $result_15;
print "Pasted columns:\n";
print $paste_result, "\n";
{
    open my $original_stdout, '>&', STDOUT or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'file1.txt' or croak "Cannot open file: $ERRNO";
    print "apple\\nbanana\\ncherry" . "\n";
    open STDOUT, '>&', $original_stdout or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
{
    open my $original_stdout, '>&', STDOUT or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'file2.txt' or croak "Cannot open file: $ERRNO";
    print "banana\\ncherry\\ndate" . "\n";
    open STDOUT, '>&', $original_stdout or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $comm_result =  my ($in_16, $out_16, $err_16); my $pid_16 = open3($in_16, $out_16, $err_16, 'comm', '-12', 'file1.txt', 'file2.txt'); close $in_16 or croak 'Close failed: $!'; my $result_16 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_16> }; close $out_16 or croak 'Close failed: $!'; waitpid $pid_16, 0; $result_16;
print "Common lines:\n";
print $comm_result, "\n";
my $diff_result =  my ($in_17, $out_17, $err_17); my $pid_17 = open3($in_17, $out_17, $err_17, 'diff', 'file1.txt', 'file2.txt'); close $in_17 or croak 'Close failed: $!'; my $result_17 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_17> }; close $out_17 or croak 'Close failed: $!'; waitpid $pid_17, 0; $result_17;
print "File differences:\n";
print $diff_result, "\n";
my $tr_result = do {
    my $output_18;
    my $pipeline_success_18 = 1;
        $output_18 .= "HELLO WORLD\n";
    my $set1_19 = 'A-Z';
    my $set2_19 = 'a-z';
    my $input_19 = $output_18;
    my $tr_result_18_1 = q{};
    for my $char ( split //msx, $input_19 ) {
        my $pos_19 = index $set1_19, $char;
        if ( $pos_19 >= 0 && $pos_19 < length $set2_19 ) {
            $tr_result_18_1 .= substr $set2_19, $pos_19, 1;
        } else {
            $tr_result_18_1 .= $char;
        }
    }
        if ( !( $tr_result_18_1 =~ /\n$/msx || $tr_result_18_1 eq q{} ) ) {
            $tr_result_18_1 .= "\n";
        }
    $output_18;
    if (!$pipeline_success_18) { $main_exit_code = 1; }
        chomp $output_18;
    $output_18 =~ s/\n/ /gsxm;
};
print "Lowercase: $tr_result\n";
my $xargs_result = do {
    my $output_20;
    my $pipeline_success_20 = 1;
        $output_20 .= "1 2 3\n";
    my ($in_21, $out_21, $err_21);
    my $pid_21 = open3($in_21, $out_21, $err_21, 'bash', '-c', 'echo "$output_20" | echo');
    close $in_21 or croak 'Close failed: $!';
    my $xargs_result_20_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_21> };
    close $out_21 or croak 'Close failed: $!';
    waitpid $pid_21, 0;
    $output_20;
    if (!$pipeline_success_20) { $main_exit_code = 1; }
        chomp $output_20;
    $output_20 =~ s/\n/ /gsxm;
};
print "Xargs result:\n";
print $xargs_result, "\n";
print "=== System Utilities ===\n";
my $formatted_date = do { use POSIX qw(strftime); strftime('%Y-%m-%d %H', localtime); };
my $timestamp = do { use POSIX qw(strftime); strftime('%rms', localtime); };
print "Timestamp: $timestamp\n";
print "Formatted date: $formatted_date\n";
my $time_result =  my ($in_22, $out_22, $err_22); my $pid_22 = open3($in_22, $out_22, $err_22, 'time', 'sleep', q{1}); close $in_22 or croak 'Close failed: $!'; my $result_22 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_22> }; close $out_22 or croak 'Close failed: $!'; waitpid $pid_22, 0; $result_22;
print "Time result: $time_result\n";
my $sleep_duration = (q{2}) . "\n";
print "Sleeping for $sleep_duration seconds...\n";
use Time::HiRes qw(sleep);
sleep $sleep_duration;
my $bash_path = do { my $command; my $found; my $result; my $dir; my $full_path; $command = bash; $found = 0; $result = q{}; foreach $dir (split /:/msx, $ENV{PATH}) { $full_path = "$dir/$command"; if (-x $full_path) { $result = $full_path; $found = 1; last; } } $result; };
print "Bash path: $bash_path\n";
my $yes_result = my $i = 0;
my $head_line_count = 0;
my $output_6 = q{};
while (1) {
    my $line = "Hello";
    $output_6 .= $line . "\n";
    if ($head_line_count < 3) {
        $output_1 .= $line . "\n";
        ++$head_line_count;
    } else {
        $line = q{}; # Clear line to prevent printing
    }
    chomp $output_6;
    $output_6 =~ s/\n/ /gsxm;
};
print "Yes command result:\n";
print $yes_result, "\n";
print "=== File Manipulation Commands ===\n";
{
    open my $original_stdout, '>&', STDOUT or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'test_file.txt' or croak "Cannot open file: $ERRNO";
    print "test content\n";
    open STDOUT, '>&', $original_stdout or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $cp_result =  my ($in_25, $out_25, $err_25); my $pid_25 = open3($in_25, $out_25, $err_25, 'cp', 'test_file.txt', 'test_file_copy.txt', '&&', 'echo', 'Copy successful'); close $in_25 or croak 'Close failed: $!'; my $result_25 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_25> }; close $out_25 or croak 'Close failed: $!'; waitpid $pid_25, 0; $result_25;
print "Copy result: $cp_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_26 = ();
if (opendir my $dh, 'test_file_moved.txt') {
    while (my $file = readdir $dh) {
        next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
        push @ls_files_26, $file;
    }
    closedir $dh;
    @ls_files_26 = sort { $a cmp $b } @ls_files_26;
}
if (@ls_files_26) {
    print join "\n", @ls_files_26, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mv_result =  my ($in_27, $out_27, $err_27); my $pid_27 = open3($in_27, $out_27, $err_27, 'mv', 'test_file_copy.txt', 'test_file_moved.txt', '&&', 'echo', 'Move successful'); close $in_27 or croak 'Close failed: $!'; my $result_27 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_27> }; close $out_27 or croak 'Close failed: $!'; waitpid $pid_27, 0; $result_27;
print "Move result: $mv_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_28 = ();
if (opendir my $dh, 'test_file_moved.txt') {
    while (my $file = readdir $dh) {
        next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
        push @ls_files_28, $file;
    }
    closedir $dh;
    @ls_files_28 = sort { $a cmp $b } @ls_files_28;
}
if (@ls_files_28) {
    print join "\n", @ls_files_28, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $rm_result =  my ($in_29, $out_29, $err_29); my $pid_29 = open3($in_29, $out_29, $err_29, 'rm', 'test_file.txt', 'test_file_moved.txt', '&&', 'echo', 'Remove successful'); close $in_29 or croak 'Close failed: $!'; my $result_29 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_29> }; close $out_29 or croak 'Close failed: $!'; waitpid $pid_29, 0; $result_29;
print "Remove result: $rm_result\n";
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_30 = ();
if (opendir my $dh, 'test_file_moved.txt') {
    while (my $file = readdir $dh) {
        next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
        push @ls_files_30, $file;
    }
    closedir $dh;
    @ls_files_30 = sort { $a cmp $b } @ls_files_30;
}
if (@ls_files_30) {
    print join "\n", @ls_files_30, "\n";
}
if ($CHILD_ERROR != 0) {
        print "No test files found\n";
}
my $mkdir_result =  my ($in_31, $out_31, $err_31); my $pid_31 = open3($in_31, $out_31, $err_31, 'mkdir', 'test_dir', '&&', 'echo', 'Directory created'); close $in_31 or croak 'Close failed: $!'; my $result_31 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_31> }; close $out_31 or croak 'Close failed: $!'; waitpid $pid_31, 0; $result_31;
print "Mkdir result: $mkdir_result\n";
use POSIX qw(time);
if (-e "test_dir/file") {
my $current_time = time;
utime $current_time, $current_time, "test_dir/file";
} else {
if (open my $fh, '>', "test_dir/file") {
close $fh or croak "Close failed: $ERRNO";
} else {
croak "touch: cannot create ", "test_dir/file", ": $ERRNO\n";
}
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
my @ls_files_33 = ();
if (opendir my $dh, 'test_dir') {
    while (my $file = readdir $dh) {
        next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
        push @ls_files_33, $file;
    }
    closedir $dh;
    @ls_files_33 = sort { $a cmp $b } @ls_files_33;
}
if (@ls_files_33) {
    print join "\n", @ls_files_33, "\n";
}
if ($CHILD_ERROR != 0) {
        print "Directory not found\n";
}
my $touch_result =  my ($in_34, $out_34, $err_34); my $pid_34 = open3($in_34, $out_34, $err_34, 'touch', 'test_file.txt', '&&', 'echo', 'File touched'); close $in_34 or croak 'Close failed: $!'; my $result_34 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_34> }; close $out_34 or croak 'Close failed: $!'; waitpid $pid_34, 0; $result_34;
print "Touch result: $touch_result\n";
print "=== Output and Formatting Commands ===\n";
my $echo_result = ('Hello from backticks') . "\n";
print "Echo result: $echo_result\n";
my $printf_result = sprintf "Number: %d  String: %s\\n", 42, test;
print "Printf result: $printf_result\n";
print "=== Compression Commands ===\n";
print "=== Network Commands ===\n";
print "=== Process Management Commands ===\n";
print "=== Checksum Commands ===\n";
{
    open my $original_stdout, '>&', STDOUT or croak "Cannot save STDOUT: $ERRNO";
    open STDOUT, '>', 'test_checksum.txt' or croak "Cannot open file: $ERRNO";
    print "test content\n";
    open STDOUT, '>&', $original_stdout or croak "Cannot restore STDOUT: $ERRNO";
    close $original_stdout or croak "Close failed: $ERRNO";
}
my $sha256_result =  my ($in_35, $out_35, $err_35); my $pid_35 = open3($in_35, $out_35, $err_35, 'sha256sum', 'test_checksum.txt'); close $in_35 or croak 'Close failed: $!'; my $result_35 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_35> }; close $out_35 or croak 'Close failed: $!'; waitpid $pid_35, 0; $result_35;
print "SHA256 result: $sha256_result\n";
my $sha512_result =  my ($in_36, $out_36, $err_36); my $pid_36 = open3($in_36, $out_36, $err_36, 'sha512sum', 'test_checksum.txt'); close $in_36 or croak 'Close failed: $!'; my $result_36 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_36> }; close $out_36 or croak 'Close failed: $!'; waitpid $pid_36, 0; $result_36;
print "SHA512 result: $sha512_result\n";
my $strings_result = my $head_line_count = 0;
while (my $line = <>) {
    chomp $line;
    my $input_data = line;
my @result;
my $current_string = q{};
for my $char (split //msx, $input_data) {
if ($char =~ /[\x20-\x7E]/msx) {
$current_string .= $char;
} else {
if (length $current_string >= 4) {
push @result, $current_string;
}
$current_string = q{};
}
}
if (length $current_string >= 4) {
push @result, $current_string;
}
my $line = join "\n", @result;

    if ($head_line_count < 3) {
    $output_1 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
}
    chomp $output_1;
    $output_1 =~ s/\n/ /gsxm;
};
print "Strings result:\n";
print $strings_result, "\n";
print "=== I/O Redirection Commands ===\n";
my $tee_result = do {
    my $output_38;
    my $pipeline_success_38 = 1;
        $output_38 .= "test output\n";
    my @lines = split /\n/msx, $output_38;
    if (open my $fh, '>', "test_tee.txt") {
    foreach my $line (@lines) {
    print {$fh} "$line\n";
    }
    close $fh or croak "Close failed: $ERRNO";
    } else {
    carp "tee: Cannot open test_tee.txt: $ERRNO";
    }
    $output_38 = join "\n", @lines;
    $output_38;
    if (!$pipeline_success_38) { $main_exit_code = 1; }
        chomp $output_38;
    $output_38 =~ s/\n/ /gsxm;
};
print "Tee result: $tee_result\n";
print "=== Perl Command ===\n";
my $perl_result =  my ($in_39, $out_39, $err_39); my $pid_39 = open3($in_39, $out_39, $err_39, 'perl', '-e', "print \"Hello from Perl\\n\""); close $in_39 or croak 'Close failed: $!'; my $result_39 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_39> }; close $out_39 or croak 'Close failed: $!'; waitpid $pid_39, 0; $result_39;
print "Perl result: $perl_result\n";
print "=== Complex Backtick Examples ===\n";
my $nested_result = ("Current time: " . (do { use POSIX qw(strftime); strftime('%a, %d %b %Y %H:%M:%S %z', localtime); })) . "\n";
print "Nested backticks: $nested_result\n";
my $count = do {
    my $output_40;
    my $pipeline_success_40 = 1;
        $output_40 = do {
        my @ls_files_41 = ();
        if (opendir my $dh, q{.}) {
            while (my $file = readdir $dh) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                push @ls_files_41, $file;
            }
            closedir $dh;
            @ls_files_41 = sort { $a cmp $b } @ls_files_41;
        }
        join "\n", @ls_files_41;
    };
    my @wc_lines_40_1 = split /\n/msx, $output_40;
    my $wc_line_count_40_1 = scalar @wc_lines_40_1;
    $output_40 = q{};
    $output_40 .= "$wc_line_count_40_1 ";
    $output_40 =~ s/\s+$//msx;
    $output_40;
    if (!$pipeline_success_40) { $main_exit_code = 1; }
        chomp $output_40;
    $output_40 =~ s/\n/ /gsxm;
};
print "File count: $count\n";
my $current_user;
$current_user =  my ($in_42, $out_42, $err_42); my $pid_42 = open3($in_42, $out_42, $err_42, 'whoami'); close $in_42 or croak 'Close failed: $!'; my $result_42 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_42> }; close $out_42 or croak 'Close failed: $!'; waitpid $pid_42, 0; $result_42;
if ("$current_user" eq "root") {
    print "Running as root\n";
}
else {
    print "Not running as root\n";
}
my $system_name;
$system_name =  my ($in_43, $out_43, $err_43); my $pid_43 = open3($in_43, $out_43, $err_43, 'uname', '-s'); close $in_43 or croak 'Close failed: $!'; my $result_43 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_43> }; close $out_43 or croak 'Close failed: $!'; waitpid $pid_43, 0; $result_43;
if ($system_name =~ /^'Linux'$/msx) {
        print "Running on Linux\n";
} elsif ($system_name =~ /^'Darwin'$/msx) {
        print "Running on macOS\n";
} elsif ($system_name =~ /^q{.*}$/msx) {
        print "Running on other system\n";
}

sub get_file_size {
    my $file = $_[0];
    my $local;
    my $size = $(...);
    print "File $file has $size bytes\n";
    return;
}
get_file_size($0);
my @files = (glob 'examples/*.sh');
print "Shell scripts found: ${scalar(@files)}\n";
for my $file (@files) {
    print "  - $file\n";
}
my $process_result =  my ($in_44, $out_44, $err_44); my $pid_44 = open3($in_44, $out_44, $err_44, 'comm', '-23'); close $in_44 or croak 'Close failed: $!'; my $result_44 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_44> }; close $out_44 or croak 'Close failed: $!'; waitpid $pid_44, 0; $result_44;
print "Process substitution result:\n";
print $process_result, "\n";
my $here_string_result =  my ($in_45, $out_45, $err_45); my $pid_45 = open3($in_45, $out_45, $err_45, 'tr', 'a-z', 'A-Z'); close $in_45 or croak 'Close failed: $!'; my $result_45 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_45> }; close $out_45 or croak 'Close failed: $!'; waitpid $pid_45, 0; $result_45;
print "Here string result: $here_string_result\n";
use File::Path qw(remove_tree);
if (-e "file1.txt") {
if (-d "file1.txt") {
carp "rm: carping: ", "file1.txt", " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "file1.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "file1.txt", ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "file1.txt", ": No such file or directory\n";
}
if (-e "file2.txt") {
if (-d "file2.txt") {
carp "rm: carping: ", "file2.txt", " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "file2.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "file2.txt", ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "file2.txt", ": No such file or directory\n";
}
if (-e "test_file.txt") {
if (-d "test_file.txt") {
carp "rm: carping: ", "test_file.txt", " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file.txt", ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file.txt", ": No such file or directory\n";
}
if (-e "test_file_copy.txt") {
if (-d "test_file_copy.txt") {
carp "rm: carping: ", "test_file_copy.txt", " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_copy.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_copy.txt", ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_copy.txt", ": No such file or directory\n";
}
if (-e "test_file_moved.txt") {
if (-d "test_file_moved.txt") {
carp "rm: carping: ", "test_file_moved.txt", " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_file_moved.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_file_moved.txt", ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_file_moved.txt", ": No such file or directory\n";
}
if (-e "test_checksum.txt") {
if (-d "test_checksum.txt") {
carp "rm: carping: ", "test_checksum.txt", " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_checksum.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_checksum.txt", ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_checksum.txt", ": No such file or directory\n";
}
if (-e "test_tee.txt") {
if (-d "test_tee.txt") {
carp "rm: carping: ", "test_tee.txt", " is a directory (use -r to remove recursively)\n";
} else {
if (unlink "test_tee.txt") {
$main_exit_code = 0;
} else {
carp "rm: carping: could not remove ", "test_tee.txt", ": $ERRNO\n";
}
}
} else {
carp "rm: carping: ", "test_tee.txt", ": No such file or directory\n";
}
open STDERR, '>', '/dev/null' or croak "Cannot open file: $ERRNO\n";
use File::Path qw(remove_tree);
if (-e "f") {
if (-d "f") {
remove_tree("f", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "f", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "f") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "f", ": $ERRNO\n";
}
}
} else {
croak "rm: ", "f", ": No such file or directory\n";
}
if (-e "test_dir") {
if (-d "test_dir") {
remove_tree("test_dir", {error => \$err});
if (@{$err}) {
croak "rm: cannot remove ", "test_dir", ": $err->[0]\n";
} else {
$main_exit_code = 0;
}
} else {
if (unlink "test_dir") {
$main_exit_code = 0;
} else {
croak "rm: cannot remove ", "test_dir", ": $ERRNO\n";
}
}
} else {
croak "rm: ", "test_dir", ": No such file or directory\n";
}
if ($CHILD_ERROR != 0) {
    system 'true';
}
print "=== Backtick Examples Complete ===\n";

exit $main_exit_code;


--- Running generated Perl code ---
Exit code: exit code: 2

==================================================
TIMING COMPARISON
==================================================
Perl execution time:  0.0998 seconds
Bash execution time:  5.7144 seconds
Perl is 57.23x faster than Bash

==================================================
OUTPUT COMPARISON
==================================================
✗ DIFFERENCES FOUND:

STDOUT DIFFERENCES:
--- bash_stdout
+++ perl_stdout
-=== Basic Backtick Usage ===
-Current date: 2025
-Current directory: /d/GitHub/sh2perl
-Stored date: 202509
-Stored directory: /d/GitHub/sh2perl
-=== File and Directory Operations ===
-File listing:
-.
-..
-.git
-__tmp_run.pl
-003
-bash_tests
-benchmark.bat
-benchmark.ps1
-BENCHMARK_README.md
-benchmark_system.pl
-build-wasm.sh
-Cargo.lock
-Cargo.toml
-clean_dead_code.sh
-clean_dead_code_safe.sh
-command_cache.json
-comment_debug.sh
-compare_perl_bash.pl
-complex_script.sh
-complex_test.sh
-create_test_data.pl
-debug_lines.txt
-docs
-echo_if_test.sh
-examples
-examples.impurl
-examples.out
-examples.pl
-fail
-fail.bat
-fail-fast
-file1.txt
-file2.txt
-find_perl_chunks.py
-find_perl_specific_code.py
-first_n_tests_passed.txt
-generated_perl.pl
-generated_perl_fixed.pl
-git_commit_sizes.pl
-git_filter_repo_generator.pl
-highest_test_count.txt
-highest_tests_and_lines.txt
-INSTALL.md
-install-deps.bat
-install-deps.ps1
-LICENSE
-ls_clean.pl
-ls_test.pl
-minimal_test.sh
-perl_functions.txt
-perl_specific_results.txt
-perlcritic_wrapper.pl
-purify.pl
-README.md
-README-WASM.md
-rename_files.sh
-restore_debug.sh
-run_benchmarks.bat
-run_benchmarks.sh
-run-next-test.bat
-simple_benchmark.pl
-simple_benchmark_no_color.pl
-simple_if_test.sh
-simple_test.sh
-src
-stderr.txt
-strip_debug.sh
-target
-test_checksum.txt
-test_dir
-test_file.txt
-test_tee.txt
-wasm.ps1
-www
-Found shell scripts:
-./bash_tests/test_local_names_preserved.sh
-./build-wasm.sh
-./clean_dead_code.sh
-./clean_dead_code_safe.sh
-./comment_debug.sh
-./complex_script.sh
-./complex_test.sh
-./echo_if_test.sh
-./examples/000__backticks.sh
-./examples/001_simple.sh
-./examples/002_control_flow.sh
-./examples/003_pipeline.sh
-./examples/004_test_quoted.sh
-./examples/005_args.sh
-./examples/006_misc.sh
-./examples/007_cat_EOF.sh
-./examples/008_simple_backup.sh
-./examples/009_arrays.sh
-./examples/010_pattern_matching.sh
-./examples/011_brace_expansion.sh
-./examples/012_process_substitution.sh
-./examples/013_parameter_expansion.sh
-./examples/014_ansi_quoting.sh
-./examples/015_grep_advanced.sh
-./examples/016_grep_basic.sh
-./examples/017_grep_context.sh
-./examples/018_grep_params.sh
-./examples/019_grep_regex.sh
-./examples/020_ansi_quoting_basic.sh
-./examples/021_ansi_quoting_escape.sh
-./examples/022_ansi_quoting_unicode.sh
-./examples/023_ansi_quoting_practical.sh
-./examples/024_parameter_expansion_case.sh
-./examples/025_parameter_expansion_advanced.sh
-./examples/026_parameter_expansion_more.sh
-./examples/027_parameter_expansion_defaults.sh
-./examples/028_arrays_indexed.sh
-./examples/029_arrays_associative.sh
-./examples/030_control_flow_if.sh
-./examples/031_control_flow_loops.sh
-./examples/032_control_flow_function.sh
-./examples/033_brace_expansion_basic.sh
-./examples/034_brace_expansion_advanced.sh
-./examples/035_brace_expansion_practical.sh
-./examples/036_pattern_matching_basic.sh
-./examples/037_pattern_matching_extglob.sh
-./examples/038_pattern_matching_nocase.sh
-./examples/039_process_substitution_here.sh
-./examples/040_process_substitution_comm.sh
-./examples/041_process_substitution_mapfile.sh
-./examples/042_process_substitution_advanced.sh
-./examples/043_home.sh
-./examples/044_find_example.sh
-./examples/045_shell_calling_perl.sh
-./examples/046_cd..sh
-./examples/047_for_arithematic.sh
-./examples/048_subprocess.sh
-./examples/049_local.sh
-./examples/050_test_ls_star_dot_sh.sh
-./examples/051_primes.sh
-./examples/052_numeric_computations.sh
-./examples/053_gcd.sh
-./examples/054_fibonacci.sh
-./examples/055_factorize.sh
-./examples/056_send_args.sh
-./examples/057_case.sh
-./examples/058_advanced_bash_idioms.sh
-./examples/059_issue3.sh
-./examples/060_issue5.sh
-./examples/061_test_local_names_preserved.sh
-./examples/062_01_ambiguous_operators.sh
-./examples/062_02_complex_parameter_expansions.sh
-./examples/062_03_complex_heredocs.sh
-./examples/062_04_nested_arithmetic.sh
-./examples/062_05_nested_command_substitution.sh
-./examples/062_06_process_substitution.sh
-./examples/062_07_complex_brace_expansion.sh
-./examples/062_08_simple_case.sh
-./examples/062_09_complex_function.sh
-./examples/062_10_simple_pipeline.sh
-./examples/062_11_mixed_arithmetic.sh
-./examples/062_12_complex_string_interpolation.sh
-./examples/062_13_simple_test_expressions.sh
-./examples/062_14_complex_array_operations.sh
-./examples/062_15_complex_local_variables.sh
-./examples/062_hard_to_lex.sh
-./examples/063_01_deeply_nested_arithmetic.sh
-./examples/063_02_complex_array_assignments.sh
-./examples/063_03_nested_command_substitutions.sh
-./examples/063_04_complex_parameter_expansion.sh
-./examples/063_05_heredoc_with_complex_content.sh
-./examples/063_06_complex_pipeline_background.sh
-./examples/063_07_nested_if_statements.sh
-./examples/063_08_complex_case_statement.sh
-./examples/063_09_complex_function_parameter_handling.sh
-./examples/063_10_complex_for_loop.sh
-./examples/063_11_complex_while_loop.sh
-./examples/063_12_complex_eval.sh
-./examples/063_13_nested_subshells.sh
-./examples/063_14_complex_redirects.sh
-./examples/063_15_complex_function_definition.sh
-./examples/063_16_complex_test_expressions.sh
-./examples/063_17_nested_brace_expansion.sh
-./examples/063_18_complex_here_string.sh
-./examples/063_19_complex_function_call.sh
-./examples/063_20_final_complex_construct.sh
-./examples/063_hard_to_parse.sh
-./examples/064_01_complex_nested_subshells.sh
-./examples/064_02_nested_brace_expansions.sh
-./examples/064_03_complex_parameter_expansion.sh
-./examples/064_04_extended_glob_patterns.sh
-./examples/064_05_complex_case_statement.sh
-./examples/064_06_nested_arithmetic_expressions.sh
-./examples/064_07_complex_array_operations.sh
-./examples/064_08_heredocs_with_variable_interpolation.sh
-./examples/064_09_process_substitution_pipeline.sh
-./examples/064_10_nested_function_definitions.sh
-./examples/064_11_complex_test_expressions.sh
-./examples/064_12_brace_expansion_nested_sequences.sh
-./examples/064_13_complex_string_manipulation.sh
-./examples/064_14_nested_command_substitution_arithmetic.sh
-./examples/064_15_complex_pipeline_multiple_redirects.sh
-./examples/064_16_function_complex_argument_handling.sh
-./examples/064_17_complex_while_loop_nested_conditionals.sh
-./examples/064_18_array_slicing_manipulation.sh
-./examples/064_19_complex_pattern_matching_extended_globs.sh
-./examples/064_20_nested_subshells_environment_variables.sh
-./examples/064_21_complex_string_interpolation_multiple_variables.sh
-./examples/064_22_function_returning_complex_data_structures.sh
-./examples/064_23_complex_error_handling_traps.sh
-./examples/064_24_advanced_parameter_expansion.sh
-./examples/064_25_complex_command_chaining.sh
-./examples/064_hard_to_generate.sh
-./examples/065_yes_head_while.sh
-./examples/900_if2echo.sh
-./examples/999_pwd.sh
-./examples/test_find.sh
-./examples/test_grep.sh
-./examples/test_perl_critic.sh
-./minimal_test.sh
-./rename_files.sh
-./restore_debug.sh
-./run_benchmarks.sh
-./simple_if_test.sh
-./simple_test.sh
-./strip_debug.sh
-./www/start_test_server.sh
-Script name: sh
-Script directory: .
-=== Text Processing Commands ===
-First 5 lines of /etc/passwd:
-
-Lines containing 'bash':
-
-Sed result: Hello Universe
-Awk sum result: 3
-Sorted words:
-apple
-banana
-zebra
-Unique words:
-apple
-banana
-cherry
-Word count: 2
-Line count: 3
-First 3 numbers: 1
-2
-3
-Last 3 numbers: 8
-9
-10
-Second field: banana
-Pasted columns:
-1\x09a
-2\x09b
-3\x09c
-Common lines:
-banana
-cherry
-File differences:
-1d0
-< apple
-3a3
-> date
-Lowercase: hello world
-Xargs result:
-Number: 1
-Number: 2
-Number: 3
-=== System Utilities ===
-Timestamp:  5:51:03 PMms
-Formatted date: 2025-09-14 17
-Time result: 
-Sleeping for 2 seconds...
-Bash path: /usr/bin/bash
-Yes command result:
-Hello
-Hello
-Hello
-=== File Manipulation Commands ===
-Copy result: Copy successful
-test_file.txt
-test_file_copy.txt
-No test files found
-Move result: Move successful
-test_file.txt
-test_file_moved.txt
-No test files found
-Remove result: Remove successful
-No test files found
-Mkdir result: 
-file
-Touch result: File touched
-=== Output and Formatting Commands ===
-Echo result: Hello from backticks
-Printf result: Number: 42, String: test
-=== Compression Commands ===
-=== Network Commands ===
-=== Process Management Commands ===
-=== Checksum Commands ===
-SHA256 result: a1fff0ffefb9eace7230c24e50731f0a91c62f9cefdfe77121c2f607125dffae *test_checksum.txt
-SHA512 result: b22137a0e8969282b85e3f9375448307d14c5aabf41be66c4f6a0323bd03a3935972021e4c34aa30914e37b03c22594fe180eea9790e9ff147016c9dfae39d5a *test_checksum.txt
-Strings result:
-!This program cannot be run in DOS mode.
-Richh
-.text
-=== I/O Redirection Commands ===
-Tee result: test output
-=== Perl Command ===


STDERR DIFFERENCES:
--- bash_stderr
+++ perl_stderr
-cat: /etc/passwd: No such file or directory
+Useless use of private variable in void context at __tmp_run.pl line 92.
-grep: /etc/passwd: No such file or directory
+Useless use of private variable in void context at __tmp_run.pl line 114.
-
+Useless use of private variable in void context at __tmp_run.pl line 136.
-real\x090m1.055s
+Useless use of private variable in void context at __tmp_run.pl line 152.
-user\x090m0.015s
+Useless use of private variable in void context at __tmp_run.pl line 174.
-sys\x090m0.030s
+Useless use of private variable in void context at __tmp_run.pl line 194.
-mkdir: cannot create directory \x2018test_dir\x2019: File exists
+Useless use of private variable in void context at __tmp_run.pl line 208.
-sh: -c: line 282: unexpected EOF while looking for matching ``'
+Useless use of private variable in void context at __tmp_run.pl line 235.
+Useless use of private variable in void context at __tmp_run.pl line 266.
+Useless use of private variable in void context at __tmp_run.pl line 287.
+Useless use of private variable in void context at __tmp_run.pl line 335.
+Useless use of private variable in void context at __tmp_run.pl line 351.
+Global symbol "$output_1" requires explicit package name (did you forget to declare "my $output_1"?) at __tmp_run.pl line 378.
+BEGIN not safe after errors--compilation aborted at __tmp_run.pl line 452.


#!/usr/bin/perl


print "=== Example 011: Basic grep command ===\n";

open(my $fh, '>', 'test_grep.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two with the word test\n";
print $fh "This is line three\n";
print $fh "Another line with test in it\n";
print $fh "This line has no matches\n";
print $fh "Final line with test pattern\n";
close($fh);

print "Using backticks to call grep:\n";
my $grep_output = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { /test/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; }
;
print $grep_output;

print "\ngrep with case insensitive (-i):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filtered_0 = grep { /-i/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
print $grep_result_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;

};

print "\ngrep with line numbers (-n):\n";
my $grep_n = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { /test/msx } @grep_lines_0;
my @grep_numbered_0;
for my $i (0..@grep_lines_0-1) {
    if (scalar grep { $_ eq $grep_lines_0[$i] } @grep_filtered_0) {
        push @grep_numbered_0, sprintf "%d:%s", $i + 1, $grep_lines_0[$i];
    }
}
$grep_result_0 = join "\n", @grep_numbered_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; }
;
print $grep_n;

print "\ngrep with count (-c):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filtered_0 = grep { /-c/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
print $grep_result_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;

};

print "\ngrep with invert match (-v):\n";
my $grep_v = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { !/test/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; }
;
print $grep_v;

print "\ngrep with word match (-w):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filtered_0 = grep { /-w/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
print $grep_result_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;

};

print "\ngrep with context (-C 1):\n";
my $grep_c = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { /test/msx } @grep_lines_0;
my @grep_with_context_0;
for my $i (0..@grep_lines_0-1) {
    if (scalar grep { $_ eq $grep_lines_0[$i] } @grep_filtered_0) {
        for my $j (($i - 1)..($i-1)) {
            if ($j >= 0) {
                push @grep_with_context_0, $grep_lines_0[$j];
            }
        }
        push @grep_with_context_0, $grep_lines_0[$i];
        for my $j (($i + 1)..($i + 1)) {
            push @grep_with_context_0, $grep_lines_0[$j];
        }
    }
}
$grep_result_0 = join "\n", @grep_with_context_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; }
;
print $grep_c;

print "\ngrep with before context (-B 2):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filtered_0 = grep { /-B/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
print $grep_result_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;

};

print "\ngrep with after context (-A 2):\n";
my $grep_a = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { /test/msx } @grep_lines_0;
my @grep_with_context_0;
for my $i (0..@grep_lines_0-1) {
    if (scalar grep { $_ eq $grep_lines_0[$i] } @grep_filtered_0) {
        push @grep_with_context_0, $grep_lines_0[$i];
        for my $j (($i + 1)..($i + 2)) {
            push @grep_with_context_0, $grep_lines_0[$j];
        }
    }
}
$grep_result_0 = join "\n", @grep_with_context_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; }
;
print $grep_a;

print "\ngrep with extended regex (-E):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filtered_0 = grep { /-E/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
print $grep_result_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;

};

print "\ngrep with fixed strings (-F):\n";
my $grep_f = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { /test/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; }
;
print $grep_f;

print "\ngrep from stdin (echo | grep):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
{
    my $output_0;
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'This is a test line' . "\n";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /test/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
    if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
    $grep_result_0_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

};

print "\ngrep with multiple files:\n";
my $grep_multi = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
if (-e "test_grep.txt") {
    open my $fh, '<', "test_grep.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "test_grep.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: test_grep.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { /test/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; }
;
print $grep_multi;

unlink('test_grep.txt') if -f 'test_grep.txt';

print "=== Example 011 completed successfully ===\n";

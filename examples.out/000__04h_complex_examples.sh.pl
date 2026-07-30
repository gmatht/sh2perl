#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
sub capture_stdout {
    my ($code) = @_;
    my $captured = q{};
    {
        local *STDOUT;
        open STDOUT, '>', \$captured
          or die "Cannot capture stdout: $OS_ERROR\n";
        $code->();
    }
    return $captured;
}

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;

$0 = '000__04h_complex_examples.sh';
my $current_user;

print "=== Complex Backtick Examples ===\n";
my $nested_result = "Three wells: " . (do { open(my $__fh, '-|', 'bash', '-c', 'yes well | head -3') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; });
print "Nested backticks: ${nested_result}\n";
my $count = do { open(my $__fh, '-|', 'bash', '-c', 'ls -1 | wc -l') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "File count: ${count}\n";
$current_user = do { my $__cs = ('root'); chomp $__cs; $__cs; };
if ($current_user eq "root") {
    print "Running as root\n";
}
else {
    print "Not running as root\n";
}
my $system_name = 'Darwin';
if ($system_name eq 'Linux') {
        print "Running on Linux\n";
} elsif ($system_name eq 'Darwin') {
        print "Running on macOS\n";
} elsif (1) {
        print "Running on other \" . \"sys\" . \"tem\n";
}

sub get_file_size {
    my ($file) = @_;
    my $size = do { my $__cs = do {
    my $wc_file = "${file}";
    my $wc_file_opened = 0;
    my $content = do {
        my $result = q{};
        if (open my $fh, '<', $wc_file) {
            $wc_file_opened = 1;
            local $INPUT_RECORD_SEPARATOR = undef;
            $result = <$fh>;
            close $fh or warn "Close failed: $OS_ERROR\n";
        } else {
            warn "Cannot open $wc_file: $OS_ERROR\n";
        }
        $result;
    };
    $wc_file_opened ? do {
        my $wc_bytes = length($content);
        $wc_bytes;
    } : q{};
}; chomp $__cs; $__cs; };
    print "File ${file} has ${size} bytes\n";
    return;
}
get_file_size(q{000__01_file_directory_operations.sh});
my @files = (do { open(my $__fh, '-|', 'bash', '-c', q{ls -1 '*.sh' 'examples/*.sh' 2> /dev/null}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; });
print("Shell scripts found: " . scalar(@files), "\n");
for my $file (@files) {
    print "  - ${file}\n";
}
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
my $process_result = do { open(my $__fh, '-|', 'bash', '-c', q{bash -c 'comm -23 <(sort file1.txt) <(sort file2.txt)'}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Process substitution result:\n";
print($process_result, "\n");
my $here_string_result = do { my $__cs = do { my $input_data = "hello world"; my $set1_111 = 'a-z';
my $set2_111 = 'A-Z';
my $input_111 = $input_data;;
print "Here string result: ${here_string_result}\n";
my $perl_result = do { my $__cs = do {
    my $result;
    my $eval_success = eval {
        $result = capture_stdout( sub { print "Hello from Perl\n" } );
        1;
    };
    if ( !$eval_success ) {
        $result = "Error executing Perl code: $EVAL_ERROR";
    }
    $result;
}; chomp $__cs; $__cs; };
print "Perl result: ${perl_result}\n";
unlink('file1.txt');
unlink('file2.txt');
print "=== Complex Backtick Examples Complete ===\n";
}
}

#!/usr/bin/env perl
use strict;
use warnings;
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

our $CHILD_ERROR;

$PROGRAM_NAME = '000__04h_complex_examples.sh';
my $current_user;

print "=== Complex Backtick Examples ===\n";
my $nested_result = "Three wells: " . (do { chomp(my $_r = qx{command yes well | head -3}); $_r; });
print "Nested backticks: $nested_result\n";
my $count = do { chomp(my $_r = qx{command ls -1 | wc -l}); $_r; };
print "File count: $count\n";
$current_user = ('root');
if ("$current_user" eq "root") {
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
        print "Running on other " . "sys" . "tem\n";
}

sub get_file_size {
    my ($file) = @_;
    my $file = $_[0];
    my $size = do {
    my $wc_file = "$file";
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
};
    print "File $file has $size bytes\n";
    return;
}
get_file_size(q{000__01_file_directory_operations.sh});
my @files = (do { my $_result = `ls -1 *.sh examples/*.sh 2>/dev/null`; chomp $_result; $CHILD_ERROR = $? >> 8; split("\n", $_result); });
print "Shell scripts found: " . scalar(@files), "\n";
for my $file (@files) {
    print "  - $file\n";
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
my $process_result = do { my @_qx_cmd = ("bash -c 'comm -23 <(sort file1.txt) <(sort file2.txt)'"); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
print "Process substitution result:\n";
print $process_result, "\n";
my $here_string_result = do { my $input_data = "hello world"; my $set1_111 = 'a-z';
my $set2_111 = 'A-Z';
my $input_111 = $input_data;;
print "Here string result: $here_string_result\n";
my $perl_result = do {
    my $result;
    my $eval_success = eval {
        $result = capture_stdout( sub { print "Hello from Perl\n" } );
        1;
    };
    if ( !$eval_success ) {
        $result = "Error executing Perl code: $EVAL_ERROR";
    }
    $result;
};
print "Perl result: $perl_result\n";
unlink('file1.txt');
unlink('file2.txt');
print "=== Complex Backtick Examples Complete ===\n";
}

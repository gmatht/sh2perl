#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;
use feature 'switch';

# DEBUG: Collected 9 variables: ["nested_result", "count", "current_user", "system_name", "files", "file", "process_result", "here_string_result", "perl_result"]
my $nested_result = 0;
my $count = 0;
my $current_user = 0;
my $system_name = 0;
my $files = 0;
my $file = 0;
my $process_result = 0;
my $here_string_result = 0;
my $perl_result = 0;

print("=== Complex Backtick Examples ===\n");
$nested_result = `echo "Three wells: \`yes well | head -3\`"`;
$ENV{nested_result} = $nested_result;
print(("Nested backticks: " . $nested_result) . "\n");
$count = `ls -1 | wc -l`;
$ENV{count} = $count;
print(("File count: " . $count) . "\n");
$current_user = `echo root`;
$ENV{current_user} = $current_user;
if (("$current_user" eq "root")) {
        print("Running as root\n");
;
} else {
        print("Not running as root\n");
;
}
$system_name = "Darwin";
$ENV{system_name} = $system_name;
given ($system_name) {
when (qr/^Linux$/) {
        print("Running on Linux\n");
    }
when (qr/^Darwin$/) {
        print("Running on macOS\n");
    }
default {
        print("Running on other system\n");
    }
}
sub get_file_size {
    my $file = "$_[0]";
    my $size = do { my $v = `wc -c < "$file"`; chomp $v; $v };
    print(("File " . $file . " has " . $size . " bytes") . "\n");
}
get_file_size("000__01_file_directory_operations.sh");
my @files = ("`ls", "-1", "*.sh", "examples/*.sh", "2>/dev/null`");
print(("Shell scripts found: " . "Shell scripts found: " . scalar(@files)) . "\n");
for $file (@files) {
    print(("  - " . $file) . "\n");
;
}
local *STDOUT; open(STDOUT, '>', 'file1.txt') or die "Cannot open file: $!\n";
print("apple\nbanana\ncherry");
local *STDOUT; open(STDOUT, '>', 'file2.txt') or die "Cannot open file: $!\n";
print("banana\ncherry\ndate");
$process_result = `comm -23 <(sort file1.txt) <(sort file2.txt)`;
$ENV{process_result} = $process_result;
print("Process substitution result:\n");
print($process_result . "\n");
$here_string_result = `tr 'a-z' 'A-Z' <<< "hello world"`;
$ENV{here_string_result} = $here_string_result;
print(("Here string result: " . $here_string_result) . "\n");
$perl_result = `perl -e 'print "Hello from Perl\n"'`;
$ENV{perl_result} = $perl_result;
print(("Perl result: " . $perl_result) . "\n");
unlink('file1.txt');
unlink('file2.txt');
print("=== Complex Backtick Examples Complete ===\n");
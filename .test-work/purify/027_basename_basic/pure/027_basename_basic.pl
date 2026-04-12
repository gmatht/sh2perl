#!/usr/bin/perl


print "=== Example 027: Basic basename command ===\n";

print "Using backticks to call basename:\n";
my $basename_output = do { my $basename_cmd = 'basename /path/to/file.txt'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
;
print "basename /path/to/file.txt: $basename_output";

print "\nbasename with suffix (remove .txt):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $basename_cmd = 'basename /path/to/file.txt .txt';
my $basename_output = qx{$basename_cmd};
$CHILD_ERROR = $? >> 8;
print $basename_output;

};

print "\nbasename with multiple suffixes:\n";
my $basename_multi = do { my $basename_cmd = 'basename /path/to/file.txt .txt .bak'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
;
print "basename /path/to/file.txt .txt .bak: $basename_multi";

print "\nbasename with zero suffix (-s ''):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $basename_cmd = 'basename -s \'\' /path/to/file.txt';
my $basename_output = qx{$basename_cmd};
$CHILD_ERROR = $? >> 8;
print $basename_output;

};

print "\nbasename with multiple paths:\n";
my $basename_paths = do { my $basename_cmd = 'basename /path/to/file1.txt /path/to/file2.txt /path/to/file3.txt'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
;
print "Multiple paths: $basename_paths";

print "\nbasename with directory:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $basename_cmd = 'basename /home/user/documents/';
my $basename_output = qx{$basename_cmd};
$CHILD_ERROR = $? >> 8;
print $basename_output;

};

print "\nbasename with current directory:\n";
my $basename_current = do { my $basename_cmd = 'basename .'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
;
print "Current directory: $basename_current";

print "\nbasename with parent directory:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $basename_cmd = 'basename ..';
my $basename_output = qx{$basename_cmd};
$CHILD_ERROR = $? >> 8;
print $basename_output;

};

print "\nbasename with root directory:\n";
my $basename_root = do { my $basename_cmd = 'basename /'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
;
print "Root directory: $basename_root";

print "\nbasename with empty string:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $basename_cmd = 'basename \'\'';
my $basename_output = qx{$basename_cmd};
$CHILD_ERROR = $? >> 8;
print $basename_output;

};

print "\nbasename with relative path:\n";
my $basename_relative = do { my $basename_cmd = 'basename ../file.txt'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
;
print "Relative path: $basename_relative";

print "\nbasename with hidden file:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $basename_cmd = 'basename /path/to/.hidden.txt';
my $basename_output = qx{$basename_cmd};
$CHILD_ERROR = $? >> 8;
print $basename_output;

};

print "\nbasename with file without extension:\n";
my $basename_no_ext = do { my $basename_cmd = 'basename /path/to/file'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }
;
print "File without extension: $basename_no_ext";

print "\nbasename with multiple extensions:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $basename_cmd = 'basename /path/to/file.txt.bak .txt.bak';
my $basename_output = qx{$basename_cmd};
$CHILD_ERROR = $? >> 8;
print $basename_output;

};

print "=== Example 027 completed successfully ===\n";

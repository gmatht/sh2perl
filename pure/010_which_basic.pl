#!/usr/bin/perl


print "=== Example 010: Basic which command ===\n";

print "Using backticks to call which:\n";
my $which_output = do { my $which_cmd = 'which ls'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; }
;
print "which ls: $which_output";

print "\nwhich with multiple commands:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $which_cmd = 'which ls cat grep';
my $which_output = qx{$which_cmd};
print $which_output;
$CHILD_ERROR = $? >> 8;

};

print "\nwhich with different commands:\n";
my @commands = qw(ls cat grep echo printf date sleep);
foreach my $cmd (@commands) {
    my $path = do { my $which_cmd = "which $cmd"; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; }
;
    chomp $path;
    if ($path) {
        print "$cmd: $path\n";
    } else {
        print "$cmd: not found\n";
    }
}

print "\nwhich with all matches (-a):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $which_cmd = 'which -a ls';
my $which_output = qx{$which_cmd};
print $which_output;
$CHILD_ERROR = $? >> 8;

};

print "\nwhich with quiet mode (-q):\n";
my $quiet_result = do { my $which_cmd = 'which -q ls'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; }
;
print "Exit code: $?\n";

print "\nwhich with version (-v):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $which_cmd = 'which -v ls';
my $which_output = qx{$which_cmd};
print $which_output;
$CHILD_ERROR = $? >> 8;

};

print "\nwhich with non-existent command:\n";
my $not_found = do { my $command = 'which nonexistentcommand 2> /dev/null'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
if ($not_found) {
    print "Found: $not_found\n";
} else {
    print "Command not found\n";
}

print "\nwhich with built-in commands:\n";
my @builtins = qw(echo printf cd);
foreach my $builtin (@builtins) {
    print "Checking $builtin: ";
    do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $which_cmd = "which $builtin";
my $which_output = qx{$which_cmd};
print $which_output;
$CHILD_ERROR = $? >> 8;

};
}

print "\nwhich with PATH modification:\n";
my $original_path = $ENV{PATH};
$ENV{PATH} = "/bin:/usr/bin";
my $path_result = do { my $which_cmd = 'which ls'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; }
;
print "ls with modified PATH: $path_result";
$ENV{PATH} = $original_path;

print "=== Example 010 completed successfully ===\n";

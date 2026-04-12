#!/usr/bin/perl


print "=== Example 002: Basic printf command ===\n";

print "Using " . "sys" . "tem" . "() to call printf:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
printf("Hello, %s!\n", "World");

};

print "\nprintf with multiple format specifiers:\n";
my $output = do {
    my $result = sprintf "Name: %s, Age: %d, Score: %.2f
", "Alice", "25", "95.5";
    $result;
}
;
print $output;

print "\nprintf with different format types:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $MAGIC_42 = 42;
printf("Integer: %d\n", "42");

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
printf("Float: %.2f\n", "3.14159");

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
printf("String: %s\n", "test");

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $MAGIC_65 = 65;
printf("Character: %c\n", ord(substr("65", 0, 1)));

};  
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $MAGIC_255 = 255;
printf("Hexadecimal: %x\n", "255");

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $MAGIC_64 = 64;
printf("Octal: %o\n", "64");

};

print "\nprintf with field width and padding:\n";
my $table_output = do {
    my $result = sprintf "%-10s %5d
", "Item1", "100";
    $result;
}
;
print $table_output;
$table_output = do {
    my $result = sprintf "%-10s %5d
", "Item2", "2000";
    $result;
}
;
print $table_output;
$table_output = do {
    my $result = sprintf "%-10s %5d
", "Item3", "30";
    $result;
}
;
print $table_output;

print "\nprintf with precision:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
printf("%.3f\n", "3.14159265359");

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
printf("%.2e\n", "1234567.89");

};

print "\nprintf with zero padding:\n";
my $padded = do {
    my $result = sprintf "%05d
", "42";
    $result;
}
;
print $padded;
$padded = do {
    my $result = sprintf "%08x
", "255";
    $result;
}
;
print $padded;

my $name = "Perl";
my $count = 42;
print "\nprintf with Perl variables:\n";
my $var_output = do {
    my $result = sprintf "Variable: %s, Count: %d
", "$name", "$count";
    $result;
}
;
print $var_output;

print "=== Example 002 completed successfully ===\n";

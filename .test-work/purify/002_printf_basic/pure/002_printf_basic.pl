#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/002_printf_basic.pl" }


print "=== Example 002: Basic printf command ===\n";

print "Using " . "sys" . "tem" . "() to call printf:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "Hello, %s!\\n", 'World'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nprintf with multiple format specifiers:\n";
my $output = do {
    my $result = sprintf "Name: %s, Age: %d, Score: %.2f
", "Alice", '25', '95.5';
    $result;
}
;
print $output;

print "\nprintf with different format types:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "Integer: %d\\n", '42'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "Float: %.2f\\n", '3.14159'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "String: %s\\n", 'test'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "Character: %c\\n", '65'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;  
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "Hexadecimal: %x\\n", '255'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "Octal: %o\\n", '64'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nprintf with field width and padding:\n";
my $table_output = do {
    my $result = sprintf "%-10s %5d
", "Item1", '100';
    $result;
}
;
print $table_output;
$table_output = do {
    my $result = sprintf "%-10s %5d
", "Item2", '2000';
    $result;
}
;
print $table_output;
$table_output = do {
    my $result = sprintf "%-10s %5d
", "Item3", '30';
    $result;
}
;
print $table_output;

print "\nprintf with precision:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "%.3f\\n", '3.14159265359'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('printf', "%.2e\\n", '1234567.89'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nprintf with zero padding:\n";
my $padded = do {
    my $result = sprintf "%05d
", '42';
    $result;
}
;
print $padded;
$padded = do {
    my $result = sprintf "%08x
", '255';
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

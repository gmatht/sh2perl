#!/usr/bin/perl

# Example 039: Subshell operations using system() and backticks
# This demonstrates subshell operations with builtins called from Perl

print "=== Example 039: Subshell operations ===\n";

# Create test files
open(my $fh, '>', 'test_subshell.txt') or die "Cannot create test file: $!\n";
print $fh "Line 1: This is a test\n";
print $fh "Line 2: Another test line\n";
print $fh "Line 3: Third test line\n";
close($fh);

# Simple subshell using backticks
print "Simple subshell using backticks:\n";
my $subshell_output = `(echo "Subshell 1"; echo "Subshell 2"; echo "Subshell 3")`;
print $subshell_output;

# Subshell with environment variables using system()
print "\nSubshell with environment variables:\n";
system("(export TEST_VAR='subshell_value'; echo \$TEST_VAR)");

# Subshell with multiple commands using backticks
print "\nSubshell with multiple commands:\n";
my $subshell_multi = `(cd .; ls -la | head -3; pwd)`;
print $subshell_multi;

# Subshell with error handling using system()
print "\nSubshell with error handling:\n";
system("(echo 'Command 1'; nonexistent_command; echo 'Command 3') 2>/dev/null || echo 'Subshell failed'");

# Subshell with pipe operations using backticks
print "\nSubshell with pipe operations:\n";
my $subshell_pipe = `(cat test_subshell.txt | grep 'test' | wc -l)`;
print "Lines with 'test': $subshell_pipe";

# Subshell with file operations using system()
print "\nSubshell with file operations:\n";
system("(touch temp1.txt; touch temp2.txt; ls temp*.txt; rm temp*.txt)");

# Subshell with conditional execution using backticks
print "\nSubshell with conditional execution:\n";
my $subshell_cond = `(test -f test_subshell.txt && echo 'File exists' || echo 'File not found')`;
print $subshell_cond;

# Subshell with background processes using system()
print "\nSubshell with background processes:\n";
#system("(sleep 1 &; sleep 1 &; wait; echo 'All background processes completed')");
system("(sleep 1 & sleep 1 & wait; echo 'All background processes completed')");

# Subshell with variable assignment using backticks
print "\nSubshell with variable assignment:\n";
my $subshell_var = `(VAR1='value1' VAR2='value2' echo "VAR1: \$VAR1, VAR2: \$VAR2")`;
print $subshell_var;

# Subshell with function calls using system()
print "\nSubshell with function calls:\n";
system("(echo 'Function call 1'; echo 'Function call 2'; echo 'Function call 3')");

# Subshell with redirection using backticks
print "\nSubshell with redirection:\n";
my $subshell_redirect = `(echo 'Output 1' > temp_output.txt; echo 'Output 2' >> temp_output.txt; cat temp_output.txt; rm temp_output.txt)`;
print $subshell_redirect;

# Subshell with complex operations using system()
print "\nSubshell with complex operations:\n";
system("(mkdir -p temp_dir; cd temp_dir; echo 'File in temp dir' > temp_file.txt; cat temp_file.txt; cd ..; rm -rf temp_dir)");

# Subshell with error propagation using backticks
print "\nSubshell with error propagation:\n";
my $subshell_error = `(echo 'Success'; false; echo 'This should not appear') 2>&1`;
print $subshell_error;

# Subshell with multiple subshells using system()
print "\nSubshell with multiple subshells:\n";
system("(echo 'Outer subshell'; (echo 'Inner subshell 1'; echo 'Inner subshell 2'); echo 'Back to outer subshell')");

# Subshell with process substitution using backticks
print "\nSubshell with process substitution:\n";
my $subshell_proc = `(echo 'Process 1'; echo 'Process 2') | cat`;
print $subshell_proc;

# Subshell with complex pipeline using system()
print "\nSubshell with complex pipeline:\n";
system("(cat test_subshell.txt | grep 'test' | sort | uniq | wc -l)");

# Clean up
unlink('test_subshell.txt') if -f 'test_subshell.txt';

print "=== Example 039 completed successfully ===\n";

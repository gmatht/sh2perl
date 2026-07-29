#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

print "=== typeset -i (integer attribute) ===\n";
delete $ENV{n};
# Builtin command 'typeset' not implemented
print "After 'typeset -i n=42': n='$ENV{n}'\n";
my $n = 'n+1';
print "After 'n=n+1':          n='$n' (integer arithmetic applied)\n";
$n = "hello";
print "After 'n=\"hello\"':     n='$n' (assigns 0; non-numeric string becomes 0)\n";
print "\n";
print "=== typeset -r (readonly attribute) ===\n";
delete $ENV{rovar};
# Builtin command 'typeset' not implemented
print "After 'typeset -r rovar=immutable': rovar='$ENV{rovar}'\n";
print "(Attempting 'rovar=change' would cause an error; skipped for safety.)\n";
print "\n";
print "=== typeset -l (lowercase attribute) ===\n";
delete $ENV{lc};
# Builtin command 'typeset' not implemented
print "After 'typeset -l lc=\"HELLO WORLD\"': lc='$ENV{lc}'\n";
my $lc = "ANOTHER TEST";
print "After 'lc=\"ANOTHER TEST\"':           lc='$lc'\n";
print "\n";
print "=== typeset -u (uppercase attribute) ===\n";
delete $ENV{uc};
# Builtin command 'typeset' not implemented
print "After 'typeset -u uc=\"hello world\"': uc='$ENV{uc}'\n";
my $uc = "another test";
print "After 'uc=\"another test\"':            uc='$uc'\n";
print "\n";
print "=== typeset -x (export attribute) ===\n";
delete $ENV{myexport};
# Builtin command 'typeset' not implemented
print "After 'typeset -x myexport=exported_value'\n";
print "Variable is exported:\n";
my $output_0 = qx{env | grep ^myexport=};
chomp $output_0;
print $output_0, "\n";
if ($CHILD_ERROR != 0) {
        print "(myexport not found in env \x{2014} possible scope issue)\n";
}
print "\n";
print "=== typeset -a (indexed array) ===\n";
delete $ENV{arr};
# Builtin command 'typeset' not implemented
print "After 'typeset -a arr=(10 20 30)': arr=(" . (join(" ", @arr)) . ")\n";
print "arr[0]='" . $arr[0] . "' arr[1]='" . $arr[1] . "' arr[2]='" . $arr[2] . "'\n";
print "\n";
print "=== typeset -A (associative array) ===\n";
delete $ENV{assoc};
# Builtin command 'typeset' not implemented
print "After 'typeset -A assoc=([key1]=value1 [key2]=value2)'\n";
print "assoc[key1]='" . $assoc[int($ENV{key1})] . "'  assoc[key2]='" . $assoc[int($ENV{key2})] . "'\n";
print "\n";
print "=== typeset -n (name reference) ===\n";
delete $ENV{original};
delete $ENV{ref};
my $original = "I am the original";
# Builtin command 'typeset' not implemented
print "After 'typeset -n ref=original':\n";
print "original='$original'\n";
print "ref='$ENV{ref}'\n";
my $ref = "Changed via ref";
print "After 'ref=\"Changed via ref\"':\n";
print "original='$original'\n";
print "ref='$ref'\n";
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
delete $ENV{-n};
undef $ref;
delete $ENV{ref};
};
print "\n";
print "=== typeset -f (display function definition) ===\n";

sub myfunc {
    print "Inside myfunc\n";
    my $x = "5";
    print "x=$x\n";
    return;
}
print "Output of 'typeset -f myfunc':\n";
# Builtin command 'typeset' not implemented
print "\n";
print "=== typeset -F (list function names) ===\n";
# Builtin command 'typeset' not implemented
print "\n";
print "=== typeset -g (global scope in function) ===\n";

sub set_global {
# Builtin command 'typeset' not implemented
    return;
}
set_global();
print "After set_global(): global_var='$ENV{global_var}'\n";
print "\n";
print "=== typeset -t (trace attribute) ===\n";
# Builtin command 'typeset' not implemented
print "After 'typeset -t tracetest=traced': (trace attribute set)\n";
print "tracetest='$ENV{tracetest}'\n";
print "\n";
print "=== typeset -p (print attribute info) ===\n";
delete $ENV{printtest};
# Builtin command 'typeset' not implemented
# Builtin command 'typeset' not implemented
print "After 'typeset -i -r printtest=99':\n";
# Builtin command 'typeset' not implemented
print "\n";
print "=== combined: typeset -il (integer + lowercase) ===\n";
delete $ENV{comb};
# Builtin command 'typeset' not implemented
print "After 'typeset -il comb=42': comb='$ENV{comb}' (integer + lowercase)\n";
my $comb = 'comb+1';
print "After 'comb=comb+1':    comb='$comb' (integer arithmetic active)\n";
print "\n";
print "=== combined: typeset -iu (integer + uppercase) ===\n";
delete $ENV{comb2};
# Builtin command 'typeset' not implemented
print "After 'typeset -iu comb2=99': comb2='$ENV{comb2}' (integer + uppercase)\n";
my $comb2 = 'comb2+1';
print "After 'comb2=comb2+1': comb2='$comb2' (integer arithmetic active)\n";
print "\n";
print "=== typeset -a (indexed array, individual element) ===\n";
delete $ENV{singlearr};
# Builtin command 'typeset' not implemented
$singlearr[0] = "first";
$singlearr[1] = "second";
print "singlearr[0]='" . $singlearr[0] . "'  singlearr[1]='" . $singlearr[1] . "'\n";
print "\n";
print "=== typeset (no flag) ===\n";
delete $ENV{plain};
# Builtin command 'typeset' not implemented
print "After 'typeset plain=\"just a string\"': plain='$ENV{plain}'\n";
print "\n";
print "=== Demonstration complete. ===\n";

exit $main_exit_code;


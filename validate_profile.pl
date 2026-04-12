#!/usr/bin/perl
use strict;
use warnings;

# Validate perltidy profile by checking each option
my @profile_lines = (
    '--indent-columns=4',
    '--maximum-line-length=120',
    '--notabs',
    '--brace-tightness=0',
    '--paren-tightness=2',
    '--square-bracket-tightness=0',
    '--cuddled-else',
    '--line-up-parentheses',
    '--break-at-old-logical-breakpoints',
    '--break-at-old-keyword-breakpoints',
    '--space-for-semicolon',
    '--nospace-function-paren',
);

print "Testing profile options...\n";
my $test_code = 'if (!($x)) { print "test"; }';
open my $fh, '>', 'test_val.pl' or die;
print $fh $test_code;
close $fh;

# Test with each option individually
for my $opt (@profile_lines) {
    my $cmd = "C:/Strawberry/perl/bin/perltidy.exe $opt --standard-output test_val.pl 2>&1";
    my $output = `$cmd`;
    my $rc = $? >> 8;
    if ($rc >= 2) {
        print "ERROR with $opt (exit code $rc):\n";
        print substr($output, 0, 100) . "\n";
    } else {
        print "OK: $opt\n";
    }
}

print "\nTesting full profile...\n";
my $profile_opts = join(' ', @profile_lines);
my $cmd = "C:/Strawberry/perl/bin/perltidy.exe $profile_opts --standard-output test_val.pl 2>&1";
my $output = `$cmd`;
my $rc = $? >> 8;
print "Exit code: $rc\n";
if ($rc >= 2) {
    print "Error output:\n$output\n";
} else {
    print "Profile works!\n";
}















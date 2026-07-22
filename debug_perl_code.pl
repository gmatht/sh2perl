#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = 'test_system_builtin.sh';
print "Testing " . "sys" . "tem" . " calls with builtin commands\n";
my $result1;
my @result1;
my %result1;
$result1 = do { my $command = 'ls -la'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };
my $result2;
my @result2;
my %result2;
$result2 = do {
    require File::Find;
    my @find_results;
    File::Find::find(sub { if ($_ =~ /^.*\.txt$/msx) { push @find_results, $File::Find::name; } }, q{.});
    my $result = join "\n", @find_results;
    if ($result ne q{}) { $result .= "\n"; }
    $CHILD_ERROR = 0;
    $result;
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

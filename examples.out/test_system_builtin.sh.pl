#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $ls_success = 0;
our $CHILD_ERROR;

print "Testing \" . \"sys\" . \"tem\" . \" calls with builtin commands\n";
my $result1 = do { my $__out = q{}; for my $__d ('-la') { opendir(my $__dh, $__d) or croak "ls: $__d: $ERRNO"; while (my $__f = readdir($__dh)) { next if $__f eq q{.} || $__f eq q{..}; my @__st = stat("$__d/$__f"); my $__mode = sprintf "%04o", $__st[2] & 4095; $__out .= sprintf("%s %3d %-8s %-8s %8d %s %s\n", $__mode, $__st[3], (getpwuid($__st[4]))[0] // $__st[4], (getgrgid($__st[5]))[0] // $__st[5], $__st[7], scalar localtime($__st[9]), $__f); } closedir($__dh); } $CHILD_ERROR = 0; q{}; };
my $result2 = do {
    require File::Find;
    my @find_results = ();
    File::Find::find(sub { if ($_ =~ /^.*\.txt$/) { push @find_results, $File::Find::name; } }, '.');
    my $result = join "\n", @find_results;
    if ($result ne '') {
        $result .= "\n";
    }
    $CHILD_ERROR = 0;
    $result;
};
print "Results:\n";
print $result1, "\n";
print $result2, "\n";


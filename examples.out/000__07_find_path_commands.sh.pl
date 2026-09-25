#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;
$0 = '000__07_find_path_commands.sh';
my $found_files = do { my $__cs = do {
    require File::Find;
    my @find_results = ();
    File::Find::find(sub { if (-f $_ && $_ =~ /^.*\.sh$/) { push @find_results, $File::Find::name; } }, '.');
    my $result = join "\n", @find_results;
    if ($result ne '') {
        $result .= "\n";
    }
    $CHILD_ERROR = 0;
    $result;
}; chomp $__cs; $__cs; };
print "Found shell scripts:\n";
print($found_files, "\n");

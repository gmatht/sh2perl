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

$PROGRAM_NAME = '000__07_find_path_commands.sh';
my $found_files;
my @found_files;
my %found_files;
$found_files = do {
    require File::Find;
    my @find_results;
    File::Find::find(sub { if (-f $_ && $_ =~ /^.*\.sh$/msx) { push @find_results, $File::Find::name; } }, q{.});
    my $result = join "\n", @find_results;
    if ($result ne q{}) { $result .= "\n"; }
    $CHILD_ERROR = 0;
    $result;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( ($found_files) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

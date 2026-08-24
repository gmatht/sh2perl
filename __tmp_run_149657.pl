#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $output = '';
our $CHILD_ERROR;
$ENV{VAR} = 'hello';
my $VAR2 = 'world';
if (!(0)) {
print("${VAR} more ${VAR2} text\n");
}
printf("%s=[%s]\n", 'VAR', (defined ($ENV{VAR} // q{}) && ($ENV{VAR} // q{}) ne q{} ? ($ENV{VAR} // q{}) : ''));
printf("%s=[%s]\n", 'VAR2', (defined ${VAR2} && ${VAR2} ne q{} ? ${VAR2} : ''));

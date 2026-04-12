#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;


sub get_file_size {
    my ($file) = @_;
    my $size = -s "$file";
    print "File $file has $size bytes\n";
    return;
}
get_file_size('test_simple_function.sh');

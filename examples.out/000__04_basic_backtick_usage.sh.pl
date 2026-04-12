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

print "=== Basic Command Substitution ===\n";
print "Current date: " . (do { use POSIX qw(strftime); strftime('%Y', localtime); }) . "\n";
print "Current directory: " . (do { use Cwd; getcwd(); }) . "\n";
my $current_dir = do { use Cwd; getcwd(); };
my $current_date = do { use POSIX qw(strftime); strftime('%Y%m', localtime); };
print "Stored date: $current_date\n";
print "Stored directory: $current_dir\n";

sub get_file_size {
    my ($file) = @_;
    my $size = -s "$file";
    print "File $file has $size bytes\n";
    return;
}
get_file_size('000__04_basic_backtick_usage.sh');
print "=== Basic Command Substitution Complete ===\n";

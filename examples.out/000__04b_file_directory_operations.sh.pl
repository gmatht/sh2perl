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
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04b_file_directory_operations.sh';
print "=== File and Directory Operations ===\n";
my $file_list = do {
    my @ls_files_45 = ();
    if ( -f q{.} ) {
        push @ls_files_45, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_45, $file;
            }
            closedir $dh;
            @ls_files_45 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_45;
        }
    }
    (@ls_files_45 ? join("\n", @ls_files_45) . "\n" : q{});
};
print "File listing:\n";
print $file_list;
if ( !( ($file_list) =~ m{\n\z}msx ) ) { print "\n"; }
my $found_files = do {
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
print "=== File and Directory Operations Complete ===\n";

exit $main_exit_code;

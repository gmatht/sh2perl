#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "=== File and Directory Operations ===\n";
my $file_list = do {
    my @ls_files_47 = ();
    if ( -f q{.} ) {
        push @ls_files_47, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_47, $file;
            }
            closedir $dh;
            @ls_files_47 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_47;
        }
    }
    (@ls_files_47 ? join("\n", @ls_files_47) . "\n" : q{});
};
print "File listing:\n";
print $file_list;
if ( !( $file_list =~ m{\n\z}msx ) ) { print "\n"; }
my $found_files = do { my $command = q{find . -name '*.sh' -type f}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }
print "=== File and Directory Operations Complete ===\n";

exit $main_exit_code;

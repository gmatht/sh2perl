#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = '000__04b_file_directory_operations.sh';
print "=== File and Directory Operations ===\n";
my $file_list = do {
    my @ls_files_46 = ();
    if ( -f q{.} ) {
        push @ls_files_46, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_46, $file;
            }
            closedir $dh;
            @ls_files_46 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_files_46;
        }
    }
    (@ls_files_46 ? join("\n", @ls_files_46) . "\n" : q{});
};
print "File listing:\n";
print $file_list, "\n";
my $found_files = do {
    require File::Find;
    my @find_results = ();
    File::Find::find(sub { if (-f $_ && $_ =~ /^.*\.sh$/) { push @find_results, $File::Find::name; } }, '.');
    my $result = join "\n", @find_results;
    if ($result ne '') {
        $result .= "\n";
    }
    $CHILD_ERROR = 0;
    $result;
};
print "Found shell scripts:\n";
print $found_files, "\n";
print "=== File and Directory Operations Complete ===\n";

#!/usr/bin/env perl
use strict;
use warnings;
print "=== Basic Command Substitution ===\n";
print "Current date: " . (do {
require POSIX; POSIX::strftime('%Y', localtime())
}), "\n";
print "Current directory: " . (do {
    my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}), "\n";
my $current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime())
};
my $current_dir = do {
    my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
};
print "Stored date: $current_date\n";
print "Stored directory: $current_dir\n";
print "=== Basic Command Substitution Complete ===\n";


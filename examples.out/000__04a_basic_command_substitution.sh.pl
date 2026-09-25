#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;
$0 = '000__04a_basic_command_substitution.sh';
print "=== Basic Command Substitution ===\n";
print("Current date: " . (do { my $__cs = do {
require POSIX; POSIX::strftime('%Y', localtime())
}; chomp $__cs; $__cs; }), "\n");
print("Current directory: " . (do { my $__cs = do {
    my $basename_path = do { my $__cs = do { use Cwd; $CHILD_ERROR = 0; getcwd(); }; chomp $__cs; $__cs; };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}; chomp $__cs; $__cs; }), "\n");
my $current_date = do { my $__cs = do {
require POSIX; POSIX::strftime('%Y%m', localtime())
}; chomp $__cs; $__cs; };
my $current_dir = do { my $__cs = do {
    my $basename_path = do { my $__cs = do { use Cwd; $CHILD_ERROR = 0; getcwd(); }; chomp $__cs; $__cs; };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}; chomp $__cs; $__cs; };
print "Stored date: ${current_date}\n";
print "Stored directory: ${current_dir}\n";
print "=== Basic Command Substitution Complete ===\n";

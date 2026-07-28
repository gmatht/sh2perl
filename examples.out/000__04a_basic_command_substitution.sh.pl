#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04a_basic_command_substitution.sh';
say "=== Basic Command Substitution ===";
say "Current date: " . (do {
require POSIX; POSIX::strftime('%Y', localtime())
});
say "Current directory: " . (do {
    my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
});
my $current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime())
};
my $current_dir = do {
    my $basename_path = do { use Cwd; $CHILD_ERROR = 0; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
};
say "Stored date: $current_date";
say "Stored directory: $current_dir";
say "=== Basic Command Substitution Complete ===";

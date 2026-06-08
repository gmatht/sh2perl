#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
my $DATE_SNAPSHOT = time;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

print "=== Basic Command Substitution ===\n";
do {
    my $output = "Current date: " . (do { my $_chomp_temp = do {
<<<<<<< HEAD
require POSIX; POSIX::strftime('%Y', localtime($DATE_SNAPSHOT)) . "\n"
=======
require POSIX; POSIX::strftime('%Y', localtime(time())) . "\n"
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
}; chomp $_chomp_temp; $_chomp_temp; });
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
<<<<<<< HEAD
    my $output = "Current directory: " . (do { my $_chomp_temp = do { my $basename_cmd = 'basename $(pwd)'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; }; chomp $_chomp_temp; $_chomp_temp; });
=======
    my $output = "Current directory: " . (do { my $_chomp_temp = do {
    my $basename_path = do { use Cwd; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}; chomp $_chomp_temp; $_chomp_temp; });
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
<<<<<<< HEAD
my $current_dir = do { my $basename_cmd = 'basename $(pwd)'; my $basename_output = qx{$basename_cmd}; $CHILD_ERROR = $? >> 8; $basename_output; };
my $current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime($DATE_SNAPSHOT)) . "\n"
=======
my $current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime(time())) . "\n"
};
my $current_dir = do {
    my $basename_path = do { use Cwd; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
};
do {
    my $output = "Stored date: $current_date";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = "Stored directory: $current_dir";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
print "=== Basic Command Substitution Complete ===\n";

exit $main_exit_code;

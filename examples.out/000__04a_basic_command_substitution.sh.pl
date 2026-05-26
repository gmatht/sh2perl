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
our $CHILD_ERROR;

print "=== Basic Command Substitution ===\n";
do {
    my $output = "Current date: " . (do { my $_chomp_temp = do {
require POSIX; POSIX::strftime('%Y', localtime(time())) . "\n"
}; chomp $_chomp_temp; $_chomp_temp; });
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = "Current directory: " . (do { my $_chomp_temp = do {
    my $basename_path = do { use Cwd; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}; chomp $_chomp_temp; $_chomp_temp; });
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $current_dir = do {
    my $basename_path = do { use Cwd; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
};
my $current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime(time())) . "\n"
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

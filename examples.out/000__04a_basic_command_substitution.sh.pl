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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04a_basic_command_substitution.sh';
print "=== Basic Command Substitution ===\n";
do {
    my $__echo_line = "Current date: " . (do { my $_chomp_temp = do {
require POSIX; POSIX::strftime('%Y', localtime(time())) . "\n"
}; chomp $_chomp_temp; $_chomp_temp; });
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = "Current directory: " . (do { my $_chomp_temp = do {
    my $basename_path = do { use Cwd; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
}; chomp $_chomp_temp; $_chomp_temp; });
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $current_date;
my @current_date;
my %current_date;
$current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime(time())) . "\n"
};
my $current_dir;
my @current_dir;
my %current_dir;
$current_dir = do {
    my $basename_path = do { use Cwd; getcwd(); };
    $basename_path =~ s{.*/}{}msx;
    chomp $basename_path;
    $basename_path;
};
do {
    my $__echo_line = "Stored date: $current_date";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = "Stored directory: $current_dir";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
print "=== Basic Command Substitution Complete ===\n";

exit $main_exit_code;

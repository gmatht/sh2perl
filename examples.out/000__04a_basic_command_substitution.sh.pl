#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "=== Basic Command Substitution ===\n";
do {
    my $output = "Current date: " . (do { my $_chomp_temp = do {
require POSIX; POSIX::strftime('%Y', localtime(time)) . "\n"
}; chomp $_chomp_temp; $_chomp_temp; });
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
do {
    my $output = "Current directory: " . (do { my $_chomp_temp = do {
    my $basename_path;
    my $basename_suffix;
    $basename_path = do { use Cwd; getcwd(); };
    $basename_suffix = q{};
    if ($basename_suffix ne q{}) {
        $basename_path =~ s/\Q$basename_suffix\E$//msx;
    }
    $basename_path =~ s/.*\///msx;
    $basename_path;
}; chomp $_chomp_temp; $_chomp_temp; });
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $current_dir = do {
    my $basename_path;
    my $basename_suffix;
    $basename_path = do { use Cwd; getcwd(); };
    $basename_suffix = q{};
    if ($basename_suffix ne q{}) {
        $basename_path =~ s/\Q$basename_suffix\E$//msx;
    }
    $basename_path =~ s/.*\///msx;
    $basename_path;
};
my $current_date = do {
require POSIX; POSIX::strftime('%Y%m', localtime(time)) . "\n"
};
do {
    my $output = "Stored date: $current_date";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
do {
    my $output = "Stored directory: $current_dir";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
print "=== Basic Command Substitution Complete ===\n";

exit $main_exit_code;

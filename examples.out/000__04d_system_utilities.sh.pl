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

print "=== System Utilities ===\n";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime(time)) . "\n"
};
do {
    my $output = "Formatted date: $formatted_date";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $sleep_duration = do {
    my $_chomp_temp = (q{1});
    chomp $_chomp_temp;
    $_chomp_temp;
};
do {
    my $output = "Sleeping for $sleep_duration seconds...";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
do {
    my $pid = fork();
    if (!defined $pid) {
        die "fork failed: $!\n";
    }
    if ($pid == 0) {
        exec 'sleep', $sleep_duration;
        exit 1;
    }
    waitpid $pid, 0;
    q{};
};
my $yes_result = do {
    my $_chomp_result = do { my $head_line_count = 0;
my $output_0 = q{};
while (1) {
    my $line = "Hello";
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_0 .= "\n"; }
    $output_0 .= $line;
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0 };
    chomp $_chomp_result;
    $_chomp_result;
};
print "Yes command result:\n";
print $yes_result;
if ( !( $yes_result =~ m{\n\z}msx ) ) { print "\n"; }
print "=== System Utilities Complete ===\n";

exit $main_exit_code;

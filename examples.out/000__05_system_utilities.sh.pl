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

print "=== System Utilities ===\n";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime(time())) . "\n"
};
do {
    my $output = "Formatted date: $formatted_date";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $yes_result = do { do {
    do { my $output_116 = q{};
my $output_printed_116;
my $head_line_count = 0;
while (1) {
    my $line = 'Hello';
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    $output_116 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_116 };
} };
print "Yes command result:\n";
print $yes_result;
if ( !( $yes_result =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

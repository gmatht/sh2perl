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

$PROGRAM_NAME = '002_control_flow.sh';
my $i;
my @i;
my %i;

my $MAGIC_10   = 10;
my $MAX_LOOP_5 = 5;

if ((-f "file.txt")) {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
for my $i ( 1 .. $MAX_LOOP_5 ) {
    do {
    my $__echo_line = "Number: $i";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
    $CHILD_ERROR = 0;
}
$i = 5;
while ( $i < $MAGIC_10 ) {
    do {
    my $__echo_line = "Counter: $i";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
    $CHILD_ERROR = 0;
    $i = eval { int($i + 1) } // "";
}

sub greet {
    my ($file) = @_;
    do {
    my $__echo_line = "Hello, $_[0]!";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
    $CHILD_ERROR = 0;
    return;
}
greet("World");

exit $main_exit_code;

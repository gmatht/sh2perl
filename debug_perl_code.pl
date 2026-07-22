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

$PROGRAM_NAME = '063_15_complex_function_definition.sh';
my $MAGIC_20 = 20;
my $MAGIC_10 = 10;
my $MAGIC_3  = 3;
my $MAGIC_7  = 7;


sub complex_func {
    my $x = "$_[0]";
    my $y = "$_[1]";
    my $result = eval { int( $x + $y ) } // "";
    do {
    my $__echo_line = "Sum: $result";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
    $CHILD_ERROR = 0;
    do {
    my $__echo_line = "Args: $x $y";
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
complex_func(q{3}, q{7});
complex_func('10', '20');

exit $main_exit_code;

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
our $CHILD_ERROR;

$PROGRAM_NAME = '064_24_advanced_parameter_expansion.sh';
my $input = (defined $_[0] && $_[0] ne q{} ? $_[0] : 'default_value');
my $sanitized = $input =~ s/\[\^a-zA-Z0-9\]/_/grs;
my $uppercase = uc(${sanitized});
do {
    my $output = "Input: '$input' -> Sanitized: '$sanitized' -> Uppercase: '$uppercase'";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;

exit $main_exit_code;

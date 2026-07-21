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

$PROGRAM_NAME = 'test_simple_function.sh';

sub get_file_size {
    my $file = $_[0];
    my $size = do {
    my $wc_file = "$file";
    open my $fh, '<', $wc_file or croak "Cannot open $wc_file: $OS_ERROR\n";
    my $content = do { local $INPUT_RECORD_SEPARATOR = undef; <$fh> };
    close $fh or croak "Close failed: $OS_ERROR\n";
    my $wc_bytes = length($content);
    $wc_bytes;
};
    do {
    my $output = "File $file has $size bytes";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
    $CHILD_ERROR = 0;
    return;
}
get_file_size('test_simple_function.sh');

exit $main_exit_code;

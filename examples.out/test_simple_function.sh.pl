#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = 'test_simple_function.sh';

sub get_file_size {
    my ($file) = @_;
    my $size = do { my $__cs = do {
    my $wc_file = "${file}";
    my $wc_file_opened = 0;
    my $content = do {
        my $result = q{};
        if (open my $fh, '<', $wc_file) {
            $wc_file_opened = 1;
            local $INPUT_RECORD_SEPARATOR = undef;
            $result = <$fh>;
            close $fh or warn "Close failed: $OS_ERROR\n";
        } else {
            warn "Cannot open $wc_file: $OS_ERROR\n";
        }
        $result;
    };
    $wc_file_opened ? do {
        my $wc_bytes = length($content);
        $wc_bytes;
    } : q{};
}; chomp $__cs; $__cs; };
    print "File ${file} has ${size} bytes\n";
    return;
}
get_file_size('test_simple_function.sh');

#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$main_exit_code = system('.', './setup-vars') >> 8;
my $left = '10';
my $unit = "seconds";
while ( (!Variable("left", false, None) eq 0) ) {
    $CHILD_ERROR = 0;
    $left = do {
    my ($in_0, $out_0);
    my $pid_0 = open3($in_0, $out_0, '>&STDERR', 'expr', $left, q{-}, q{1});
    close $in_0 or croak 'Close failed: $OS_ERROR';
    my $result_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_0> };
    close $out_0 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_0, 0;
    $result_0
};
    if (do {
$main_exit_code = system('test', "$left", q{=}, q{1}) >> 8;
        $CHILD_ERROR == 0
    }) {
                $unit = "second";
    }
}

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = 'zstd_example.sh';
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/rp2.fw'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {

${} = do { open(my $__fh, '-|', 'zstd', '-d', q{c}, '/usr/lib/firmware/rp2.fw.zst') or croak "failed: $ERRNO"; chomp(my $_r = do { local $/; <$__fh> }); close $__fh; $_r; };
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
print "Decompressed rp2 firmware\n";
print("exit: " . ($? >> 8), "\n");

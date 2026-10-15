#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = 'gunzip_example.sh';
open STDIN, '<', '/usr/share/man/uk/man1/w.1.gz' or croak "Cannot read file: $OS_ERROR\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/w.1'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {

${} = do { open(my $__fh, '-|', 'gunzip') or croak "failed: $ERRNO"; chomp(my $_r = do { local $/; <$__fh> }); close $__fh; $_r; };
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
print "Decompressed man page\n";
print("exit: " . ($? >> 8), "\n");

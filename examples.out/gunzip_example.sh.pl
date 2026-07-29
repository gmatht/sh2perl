#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = 'gunzip_example.sh';
open STDIN, '<', '/usr/share/man/uk/man1/w.1.gz' or croak "Cannot read file: $OS_ERROR\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/w.1'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {

my @_qx_cmd_455 = ('command gunzip');
${} = do { chomp(my $_r_455 = qx{command $_qx_cmd_455[0]}); $_r_455; };
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
print "Decompressed man page\n";
print "exit: ${\($? >> 8)}\n";

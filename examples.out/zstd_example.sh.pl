#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = 'zstd_example.sh';
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/rp2.fw'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {

my @_qx_cmd_613 = ('command zstd -dc /usr/lib/firmware/rp2.fw.zst');
${} = do { chomp(my $_r_613 = qx{command $_qx_cmd_613[0]}); $_r_613; };
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
print "Decompressed rp2 firmware\n";
print "exit: ${\($? >> 8)}\n";

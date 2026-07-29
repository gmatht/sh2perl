#!/usr/bin/env perl
use strict;
use warnings;
open STDIN, '<', '/usr/share/man/uk/man1/w.1.gz' or croak "Cannot read file: $OS_ERROR\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/w.1'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    $main_exit_code = system('bash', 'gunzip') >> 8;
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
print "Decompressed man page\n";


#!/usr/bin/env perl
use strict;
use warnings;
use File::Find;

my @test_files = (
    "./build-wasm.sh",
    "./examples/000__01_file_directory_operations.sh",
    "./bash_tests/test_local_names_preserved.sh",
    "./www/start_test_server.sh"
);

for my $file (@test_files) {
    print "File: $file\n";
    print "  -f: " . (-f $file ? "true" : "false") . "\n";
    print "  -e: " . (-e $file ? "true" : "false") . "\n";
    print "  -r: " . (-r $file ? "true" : "false") . "\n";
    print "  -T: " . (-T $file ? "true" : "false") . "\n";
    print "  -B: " . (-B $file ? "true" : "false") . "\n";
    print "\n";
}

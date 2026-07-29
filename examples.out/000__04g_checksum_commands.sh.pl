#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;
use Digest::SHA   qw(sha256_hex sha512_hex);
use File::Path    qw(make_path remove_tree);
our $CHILD_ERROR;

print "=== Checksum Commands ===\n";
open my $fh, '>', 'test_checksum.txt' or die "test_checksum.txt: $!\n";
print {*fh} "test content", "\n";
close $fh;
my $sha256_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha256_hex(
            do {
                local $/ = undef;
                open my $fh, '<', 'test_checksum.txt'
                  or croak "Cannot open 'test_checksum.txt': $!";
                my $content = <$fh>;
                close $fh
                  or croak "Close failed: $!";
                $content;
            }
        );
        push @results, "$hash  test_checksum.txt";
    }
    else {
        push @results,
"0000000000000000000000000000000000000000000000000000000000000000  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
print "SHA256 result: $sha256_result\n";
my $sha512_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha512_hex(
            do {
                local $/ = undef;
                open my $fh, '<', 'test_checksum.txt'
                  or croak "Cannot open 'test_checksum.txt': $!";
                my $content = <$fh>;
                close $fh
                  or croak "Close failed: $!";
                $content;
            }
        );
        push @results, "$hash  test_checksum.txt";
    }
    else {
        push @results,
"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
print "SHA512 result: $sha512_result\n";
my $strings_result = do { chomp(my $_r = qx{command strings target/debug/debashc.exe | head -3}); $_r; };
print "Strings result:\n";
print $strings_result, "\n";
unlink('test_checksum.txt');
print "=== Checksum Commands Complete ===\n";


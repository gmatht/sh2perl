#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use Digest::SHA   qw(sha256_hex sha512_hex);
use File::Path    qw(make_path remove_tree);

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04g_checksum_commands.sh';
say "=== Checksum Commands ===";
open my $fh, '>', 'test_checksum.txt' or die "test_checksum.txt: $!\n";
say {*fh} "test content";
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
say "SHA256 result: $sha256_result";
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
say "SHA512 result: $sha512_result";
my $strings_result = do { chomp(my $result_107 = qx{strings target/debug/debashc.exe | head -3}); $result_107; };
say "Strings result:";
say $strings_result;
unlink('test_checksum.txt');
say "=== Checksum Commands Complete ===";

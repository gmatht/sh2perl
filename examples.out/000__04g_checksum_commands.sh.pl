#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use Digest::SHA   qw(sha256_hex sha512_hex);
use File::Path    qw(make_path remove_tree);
my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;
$0 = '000__04g_checksum_commands.sh';
print "=== Checksum Commands ===\n";
open my $fh, '>', 'test_checksum.txt' or die "test_checksum.txt: $!\n";
print {$fh}("test content", "\n");
close $fh;
my $sha256_result = do { my $__cs = do {
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
; chomp $__cs; $__cs; };
print "SHA256 result: ${sha256_result}\n";
my $sha512_result = do { my $__cs = do {
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
; chomp $__cs; $__cs; };
print "SHA512 result: ${sha512_result}\n";
my $strings_result = do { open(my $__fh, '-|', 'bash', '-c', 'strings target/debug/debashc.exe | head -3') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Strings result:\n";
print($strings_result, "\n");
unlink('test_checksum.txt');
print "=== Checksum Commands Complete ===\n";

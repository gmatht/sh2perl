#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use Digest::SHA   qw(sha256_hex sha512_hex);
use File::Path    qw(make_path remove_tree);
sub capture_stdout {
    my ($code) = @_;
    my $captured = q{};
    {
        local *STDOUT;
        open STDOUT, '>', \$captured
          or die "Cannot capture stdout: $OS_ERROR\n";
        $code->();
    }
    return $captured;
}

our $CHILD_ERROR;

$0 = '000__02_output_formatting_commands.sh';
print "=== Output and Formatting Commands ===\n";
my $echo_result = "Hello from backticks";
print "Echo result: $echo_result\n";
my $printf_result = sprintf("Number: %d, String: %s\n", 42, "test");
print "Printf result: $printf_result\n";
print "=== Compression Commands ===\n";
print "=== Network Commands ===\n";
print "=== Process Management Commands ===\n";
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
my $strings_result = do { open(my $__fh, '-|', 'bash', '-c', 'strings test_binary.txt | head -3') or croak "cmd failed: $!"; local $/; chomp(my $_r = <$__fh>); close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "Strings result:\n";
print $strings_result, "\n";
print "=== I/O Redirection Commands ===\n";
my $tee_result = do { open(my $__fh, '-|', 'bash', '-c', 'echo \'test output\' | tee test_tee.txt') or croak "cmd failed: $!"; local $/; chomp(my $_r = <$__fh>); close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "Tee result: $tee_result\n";
print "=== Perl Command ===\n";
my $perl_result = do {
    my $result;
    my $eval_success = eval {
        $result = capture_stdout( sub { print "Hello from Perl\n" } );
        1;
    };
    if ( !$eval_success ) {
        $result = "Error executing Perl code: $EVAL_ERROR";
    }
    $result;
};
print "Perl result: $perl_result\n";
unlink('test_checksum.txt');
unlink('test_tee.txt');

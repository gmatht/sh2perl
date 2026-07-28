#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use Carp;
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


my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__02_output_formatting_commands.sh';
say "=== Output and Formatting Commands ===";
my $echo_result = "Hello from backticks";
say "Echo result: $echo_result";
my $printf_result = sprintf("Number: %d, String: %s\n", 42, "test");
say "Printf result: $printf_result";
say "=== Compression Commands ===";
say "=== Network Commands ===";
say "=== Process Management Commands ===";
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
my $strings_result = do { chomp(my $result_0 = qx{strings test_binary.txt | head -3}); $result_0; };
say "Strings result:";
say $strings_result;
say "=== I/O Redirection Commands ===";
my $tee_result = do { chomp(my $result_1 = qx{echo 'test output' | tee test_tee.txt}); $result_1; };
say "Tee result: $tee_result";
say "=== Perl Command ===";
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
say "Perl result: $perl_result";
unlink('test_checksum.txt');
unlink('test_tee.txt');

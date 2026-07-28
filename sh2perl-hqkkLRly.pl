#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;


sub decrypt_gpg {
    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say "Performing GPG symmetric decryption ...";
    };
if (    # Original bash: /lib/cryptsetup/askpass "Enter passphrase for key $1: " | \
do {
        my $output_0 = q{};
        my $output_printed_0;
        my $pipeline_success_0 = 1;
                my ($in_1, $out_1);
        my $pid_1 = open3($in_1, $out_1, '>&STDERR', '/lib/cryptsetup/askpass', );
        close $in_1 or croak 'Close failed: $OS_ERROR';
        $output_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_1> };
        close $out_1 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_1, 0;

                my $cmd_3 = '/usr/bin/gpg';
        my ($in_2, $out_2);
        my $pid_2 = open3($in_2, $out_2, '>&STDERR', $cmd_3, '-q', '--batch', '--no-options', '--no-random-seed-file', '--no-default-keyring', '--keyring', '/dev/null', '--secret-keyring', '/dev/null', '--trustdb-name', '/dev/null', '--passphrase-fd', q{0}, '--decrypt', '--');
        print {$in_2} $output_0;
        close $in_2 or croak 'Close failed: $OS_ERROR';
        $output_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_2> };
        close $out_2 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_2, 0;
        if ($output_0 ne q{} && !defined $output_printed_0) {
            print $output_0;
            if (!($output_0 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
        }) {
return q{1};
    }
return q{0};
    return;
}
if ((!-x /usr/bin/gpg)) {
    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say "$PROGRAM_NAME: /usr/bin/gpg is not available";
    };
exit 1;
}
if ("$1" eq q{}) {
    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say "$PROGRAM_NAME: missing key as argument";
    };
exit 1;
}
decrypt_gpg("$_[0]");


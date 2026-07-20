#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = 'test_system_builtin.sh';
print "Testing " . "sys" . "tem" . " calls with builtin commands\n";
my $result2 = do { my $command = q{find . -name '*.txt'}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };
my $result1 = do {
    my @ls_files_345 = ();
    if ( -f q{.} ) {
        push @ls_files_345, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_345, $file;
            }
            closedir $dh;
            @ls_files_345 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_345;
        }
    }
    @ls_files_345 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_345;
    (@ls_files_345 ? join("\n", @ls_files_345) . "\n" : q{});
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

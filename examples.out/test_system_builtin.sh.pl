#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "Testing " . "sys" . "tem" . " calls with builtin commands\n";
my $result1;
$result1 = do {
    my @ls_files_298 = ();
    if ( -f q{.} ) {
        push @ls_files_298, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_298, $file;
            }
            closedir $dh;
            @ls_files_298 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_298;
        }
    }
    @ls_files_298 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_298;
    (@ls_files_298 ? join("\n", @ls_files_298) . "\n" : q{});
};
my $result2;
$result2 = do {
    use File::Find;
    use File::Basename;
    my @files_300 = ();
    my $start_300 = q{.};

    find( sub {
        my $file_300 = $File::Find::name;
        if ( !( basename($file_300) =~ m/^.*.txt$/xms ) ) {
            return;
        }
        push @files_300, $file_300;
    },
    $start_300 );
    join "\n", @files_300;
};
print "Results:\n";
print $result1;
if ( !( $result1 =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( $result2 =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

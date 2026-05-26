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
my $result2 = do {
    use File::Basename;
    my @files_0 = ();
    my $start_0 = q{.};
    my $_find_0;
    $_find_0 = sub {
        my ($dir_0, $depth_0) = @_;
        opendir(my $dh_0, $dir_0) or return;
        my @entries_0 = readdir($dh_0);
        closedir($dh_0);
        for my $entry_0 (@entries_0) {
            next if $entry_0 eq q{.} || $entry_0 eq q{..};
            my $file_0 = "$dir_0/$entry_0";
            if (-d $file_0) {
                $_find_0->($file_0, $depth_0 + 1);
            }
            elsif (-f $file_0) {
                next if !( basename($file_0) =~ m/^.*.txt$/xms );
                push @files_0, $file_0;
            }
        }
    };
    $_find_0->($start_0, 0);
    join "\n", @files_0;
};
my $result1 = do {
    my @ls_files_1 = ();
    if ( -f q{.} ) {
        push @ls_files_1, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_1, $file;
            }
            closedir $dh;
            @ls_files_1 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_1;
        }
    }
    @ls_files_1 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_1;
    (@ls_files_1 ? join("\n", @ls_files_1) . "\n" : q{});
};
print "Results:\n";
print $result1;
if ( !( $result1 =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( $result2 =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

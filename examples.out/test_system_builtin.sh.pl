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

print "Testing " . "sys" . "tem" . " calls with builtin commands\n";
my $result1 = do {
    my @ls_files_290 = ();
    if ( -f q{.} ) {
        push @ls_files_290, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_290, $file;
            }
            closedir $dh;
            @ls_files_290 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_290;
        }
    }
    @ls_files_290 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_290;
    (@ls_files_290 ? join("\n", @ls_files_290) . "\n" : q{});
};
my $result2 = do {
    use File::Basename;
    my @files_292 = ();
    my $start_292 = q{.};
    my $_find_292;
    $_find_292 = sub {
        my ($dir_292, $depth_292) = @_;
        opendir(my $dh_292, $dir_292) or return;
        my @entries_292 = readdir($dh_292);
        closedir($dh_292);
        for my $entry_292 (@entries_292) {
            next if $entry_292 eq q{.} || $entry_292 eq q{..};
            my $file_292 = "$dir_292/$entry_292";
            if (-d $file_292) {
                $_find_292->($file_292, $depth_292 + 1);
            }
            elsif (-f $file_292) {
                next if !( basename($file_292) =~ m/^.*.txt$/xms );
                push @files_292, $file_292;
            }
        }
    };
    $_find_292->($start_292, 0);
    join "\n", @files_292;
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

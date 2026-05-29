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
    my @ls_files_277 = ();
    if ( -f q{.} ) {
        push @ls_files_277, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_277, $file;
            }
            closedir $dh;
            @ls_files_277 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_277;
        }
    }
    @ls_files_277 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_277;
    (@ls_files_277 ? join("\n", @ls_files_277) . "\n" : q{});
};
my $result2 = do {
    use File::Basename;
    my @files_279 = ();
    my $start_279 = q{.};
    my $_find_279;
    $_find_279 = sub {
        my ($dir_279, $depth_279) = @_;
        opendir(my $dh_279, $dir_279) or return;
        my @entries_279 = readdir($dh_279);
        closedir($dh_279);
        for my $entry_279 (@entries_279) {
            next if $entry_279 eq q{.} || $entry_279 eq q{..};
            my $file_279 = "$dir_279/$entry_279";
            if (-d $file_279) {
                $_find_279->($file_279, $depth_279 + 1);
            }
            elsif (-f $file_279) {
                next if !( basename($file_279) =~ m/^.*.txt$/xms );
                push @files_279, $file_279;
            }
        }
    };
    $_find_279->($start_279, 0);
    join "\n", @files_279;
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

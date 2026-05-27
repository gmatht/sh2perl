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
my $result2 = do {
    use File::Basename;
    my @files_277 = ();
    my $start_277 = q{.};
    my $_find_277;
    $_find_277 = sub {
        my ($dir_277, $depth_277) = @_;
        opendir(my $dh_277, $dir_277) or return;
        my @entries_277 = readdir($dh_277);
        closedir($dh_277);
        for my $entry_277 (@entries_277) {
            next if $entry_277 eq q{.} || $entry_277 eq q{..};
            my $file_277 = "$dir_277/$entry_277";
            if (-d $file_277) {
                $_find_277->($file_277, $depth_277 + 1);
            }
            elsif (-f $file_277) {
                next if !( basename($file_277) =~ m/^.*.txt$/xms );
                push @files_277, $file_277;
            }
        }
    };
    $_find_277->($start_277, 0);
    join "\n", @files_277;
};
my $result1 = do {
    my @ls_files_278 = ();
    if ( -f q{.} ) {
        push @ls_files_278, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_278, $file;
            }
            closedir $dh;
            @ls_files_278 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_278;
        }
    }
    @ls_files_278 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_278;
    (@ls_files_278 ? join("\n", @ls_files_278) . "\n" : q{});
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

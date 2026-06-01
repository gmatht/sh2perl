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
    my @ls_files_285 = ();
    if ( -f q{.} ) {
        push @ls_files_285, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_285, $file;
            }
            closedir $dh;
            @ls_files_285 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_285;
        }
    }
    @ls_files_285 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_285;
    (@ls_files_285 ? join("\n", @ls_files_285) . "\n" : q{});
};
my $result2 = do {
    use File::Basename;
    my @files_287 = ();
    my $start_287 = q{.};
    my $_find_287;
    $_find_287 = sub {
        my ($dir_287, $depth_287) = @_;
        opendir(my $dh_287, $dir_287) or return;
        my @entries_287 = readdir($dh_287);
        closedir($dh_287);
        for my $entry_287 (@entries_287) {
            next if $entry_287 eq q{.} || $entry_287 eq q{..};
            my $file_287 = "$dir_287/$entry_287";
            if (-d $file_287) {
                $_find_287->($file_287, $depth_287 + 1);
            }
            elsif (-f $file_287) {
                next if !( basename($file_287) =~ m/^.*.txt$/xms );
                push @files_287, $file_287;
            }
        }
    };
    $_find_287->($start_287, 0);
    join "\n", @files_287;
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

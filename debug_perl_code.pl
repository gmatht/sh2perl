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
    my @files_290 = ();
    my $start_290 = q{.};
    my $_find_290;
    $_find_290 = sub {
        my ($dir_290, $depth_290) = @_;
        opendir(my $dh_290, $dir_290) or return;
        my @entries_290 = readdir($dh_290);
        closedir($dh_290);
        for my $entry_290 (@entries_290) {
            next if $entry_290 eq q{.} || $entry_290 eq q{..};
            my $file_290 = "$dir_290/$entry_290";
            if (-d $file_290) {
                $_find_290->($file_290, $depth_290 + 1);
            }
            elsif (-f $file_290) {
                next if !( basename($file_290) =~ m/^.*.txt$/xms );
                push @files_290, $file_290;
            }
        }
    };
    $_find_290->($start_290, 0);
    join "\n", @files_290;
};
my $result1 = do {
    my @ls_files_291 = ();
    if ( -f q{.} ) {
        push @ls_files_291, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_291, $file;
            }
            closedir $dh;
            @ls_files_291 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_291;
        }
    }
    @ls_files_291 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_291;
    (@ls_files_291 ? join("\n", @ls_files_291) . "\n" : q{});
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

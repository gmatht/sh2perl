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
    my @ls_files_282 = ();
    if ( -f q{.} ) {
        push @ls_files_282, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_282, $file;
            }
            closedir $dh;
            @ls_files_282 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_282;
        }
    }
    @ls_files_282 = map { my $isdir = (-d $_ || -d ( q{.} . q{/} . $_ )); ($isdir ? 'd ' : '- ') . $_ } @ls_files_282;
    (@ls_files_282 ? join("\n", @ls_files_282) . "\n" : q{});
};
my $result2 = do {
    use File::Basename;
    my @files_284 = ();
    my $start_284 = q{.};
    my $_find_284;
    $_find_284 = sub {
        my ($dir_284, $depth_284) = @_;
        opendir(my $dh_284, $dir_284) or return;
        my @entries_284 = readdir($dh_284);
        closedir($dh_284);
        for my $entry_284 (@entries_284) {
            next if $entry_284 eq q{.} || $entry_284 eq q{..};
            my $file_284 = "$dir_284/$entry_284";
            if (-d $file_284) {
                $_find_284->($file_284, $depth_284 + 1);
            }
            elsif (-f $file_284) {
                next if !( basename($file_284) =~ m/^.*.txt$/xms );
                push @files_284, $file_284;
            }
        }
    };
    $_find_284->($start_284, 0);
    join "\n", @files_284;
};
print "Results:\n";
print $result1;
if ( !( ($result1) =~ m{\n\z}msx ) ) { print "\n"; }
print $result2;
if ( !( ($result2) =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

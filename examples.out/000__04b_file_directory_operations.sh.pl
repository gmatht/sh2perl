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

print "=== File and Directory Operations ===\n";
my $file_list = do {
    my @ls_files_48 = ();
    if ( -f q{.} ) {
        push @ls_files_48, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_48, $file;
            }
            closedir $dh;
            @ls_files_48 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_48;
        }
    }
    (@ls_files_48 ? join("\n", @ls_files_48) . "\n" : q{});
};
print "File listing:\n";
print $file_list;
if ( !( $file_list =~ m{\n\z}msx ) ) { print "\n"; }
my $found_files = do {
    use File::Basename;
    my @files_50 = ();
    my $start_50 = q{.};
    my $_find_50;
    $_find_50 = sub {
        my ($dir_50, $depth_50) = @_;
        opendir(my $dh_50, $dir_50) or return;
        my @entries_50 = readdir($dh_50);
        closedir($dh_50);
        for my $entry_50 (@entries_50) {
            next if $entry_50 eq q{.} || $entry_50 eq q{..};
            my $file_50 = "$dir_50/$entry_50";
            if (-d $file_50) {
                $_find_50->($file_50, $depth_50 + 1);
            }
            elsif (-f $file_50) {
                next if !( basename($file_50) =~ m/^.*.sh$/xms );
                push @files_50, $file_50;
            }
        }
    };
    $_find_50->($start_50, 0);
    join "\n", @files_50;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }
print "=== File and Directory Operations Complete ===\n";

exit $main_exit_code;

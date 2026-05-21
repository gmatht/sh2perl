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
    my @ls_files_0 = ();
    if ( -f q{.} ) {
        push @ls_files_0, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_0, $file;
            }
            closedir $dh;
            @ls_files_0 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_0;
        }
    }
    (@ls_files_0 ? join("\n", @ls_files_0) . "\n" : q{});
};
print "File listing:\n";
print $file_list;
if ( !( $file_list =~ m{\n\z}msx ) ) { print "\n"; }
my $found_files = do {
    use File::Basename;
    my @files_2 = ();
    my $start_2 = q{.};
    my $_find_2;
    $_find_2 = sub {
        my ($dir_2, $depth_2) = @_;
        return if $depth_2 > 1;
        opendir(my $dh_2, $dir_2) or return;
        my @entries_2 = readdir($dh_2);
        closedir($dh_2);
        for my $entry_2 (@entries_2) {
            next if $entry_2 eq q{.} || $entry_2 eq q{..};
            my $file_2 = "$dir_2/$entry_2";
            if (-d $file_2) {
                $_find_2->($file_2, $depth_2 + 1);
            }
            elsif (-f $file_2) {
                next if !( basename($file_2) =~ m/^.*.sh$/xms );
                push @files_2, $file_2;
            }
        }
    };
    $_find_2->($start_2, 0);
    join "\n", @files_2;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

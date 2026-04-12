#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
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
            @ls_files_48 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_48;
        }
    }
    (@ls_files_48 ? join("\n", @ls_files_48) . "\n" : q{});
};
print "File listing:\n";
print $file_list;
if ( !( $file_list =~ m{\n\z}msx ) ) { print "\n"; }
my $found_files = do {
    use File::Find;
    use File::Basename;
    my @files_50 = ();
    my $start_50 = q{.};

    sub find_files_50 {
        my $file_50 = $File::Find::name;
        if ( !( -f $file_50 ) ) {
            return;
        }
        if ( !( basename($file_50) =~ m/^.*.sh$/xms ) ) {
            return;
        }
        push @files_50, $file_50;
        return;
    }
    find( \&find_files_50, $start_50 );
    join "\n", @files_50;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }
print "=== File and Directory Operations Complete ===\n";

exit $main_exit_code;

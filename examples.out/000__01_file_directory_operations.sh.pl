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
            @ls_files_0 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_0;
        }
    }
    (@ls_files_0 ? join("\n", @ls_files_0) . "\n" : q{});
};
print "File listing:\n";
print $file_list;
if ( !( $file_list =~ m{\n\z}msx ) ) { print "\n"; }
my $found_files = do {
    use File::Find;
    use File::Basename;
    my @files_2 = ();
    my $start_2 = q{.};

    sub find_files_2 {
        my $file_2 = $File::Find::name;
        if ( !( -f $file_2 ) ) {
            return;
        }
        if ( !( basename($file_2) =~ m/^.*.sh$/xms ) ) {
            return;
        }
        push @files_2, $file_2;
        return;
    }
    find( \&find_files_2, $start_2 );
    join "\n", @files_2;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

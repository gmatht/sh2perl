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

my $found_files = do {
    use File::Find;
    use File::Basename;
    my @files_145 = ();
    my $start_145 = q{.};

    sub find_files_145 {
        my $file_145 = $File::Find::name;
        if ( !( -f $file_145 ) ) {
            return;
        }
        if ( !( basename($file_145) =~ m/^.*.sh$/xms ) ) {
            return;
        }
        push @files_145, $file_145;
        return;
    }
    find( \&find_files_145, $start_145 );
    join "\n", @files_145;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

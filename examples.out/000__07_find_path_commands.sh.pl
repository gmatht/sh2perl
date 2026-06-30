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

my $found_files;
$found_files = do {
    use File::Find;
    use File::Basename;
    my @files_137 = ();
    my $start_137 = q{.};

    find( sub {
        my $file_137 = $File::Find::name;
        if ( !( -f $file_137 ) ) {
            return;
        }
        if ( !( basename($file_137) =~ m/^.*.sh$/xms ) ) {
            return;
        }
        push @files_137, $file_137;
    },
    $start_137 );
    join "\n", @files_137;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

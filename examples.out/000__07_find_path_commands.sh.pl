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

my $found_files = do {
    use File::Basename;
    my @files_138 = ();
    my $start_138 = q{.};
    my $_find_138;
    $_find_138 = sub {
        my ($dir_138, $depth_138) = @_;
        opendir(my $dh_138, $dir_138) or return;
        my @entries_138 = readdir($dh_138);
        closedir($dh_138);
        for my $entry_138 (@entries_138) {
            next if $entry_138 eq q{.} || $entry_138 eq q{..};
            my $file_138 = "$dir_138/$entry_138";
            if (-d $file_138) {
                $_find_138->($file_138, $depth_138 + 1);
            }
            elsif (-f $file_138) {
                next if !( basename($file_138) =~ m/^.*.sh$/xms );
                push @files_138, $file_138;
            }
        }
    };
    $_find_138->($start_138, 0);
    join "\n", @files_138;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

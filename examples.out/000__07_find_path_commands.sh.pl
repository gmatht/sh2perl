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

my $found_files = do {
    use File::Basename;
    my @files_135 = ();
    my $start_135 = q{.};
    my $_find_135;
    $_find_135 = sub {
        my ($dir_135, $depth_135) = @_;
        opendir(my $dh_135, $dir_135) or return;
        my @entries_135 = readdir($dh_135);
        closedir($dh_135);
        for my $entry_135 (@entries_135) {
            next if $entry_135 eq q{.} || $entry_135 eq q{..};
            my $file_135 = "$dir_135/$entry_135";
            if (-d $file_135) {
                $_find_135->($file_135, $depth_135 + 1);
            }
            elsif (-f $file_135) {
                next if !( basename($file_135) =~ m/^.*.sh$/xms );
                push @files_135, $file_135;
            }
        }
    };
    $_find_135->($start_135, 0);
    join "\n", @files_135;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( $found_files =~ m{\n\z}msx ) ) { print "\n"; }

exit $main_exit_code;

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

print "=== File and Directory Operations ===\n";
my $file_list = do {
    my @ls_files_45 = ();
    if ( -f q{.} ) {
        push @ls_files_45, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_45, $file;
            }
            closedir $dh;
            @ls_files_45 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_45;
        }
    }
    (@ls_files_45 ? join("\n", @ls_files_45) . "\n" : q{});
};
print "File listing:\n";
print $file_list;
if ( !( ($file_list) =~ m{\n\z}msx ) ) { print "\n"; }
my $found_files = do {
    use File::Basename;
    my @files_47 = ();
    my $start_47 = q{.};
    my $_find_47;
    $_find_47 = sub {
        my ($dir_47, $depth_47) = @_;
        opendir(my $dh_47, $dir_47) or return;
        my @entries_47 = readdir($dh_47);
        closedir($dh_47);
        for my $entry_47 (@entries_47) {
            next if $entry_47 eq q{.} || $entry_47 eq q{..};
            my $file_47 = "$dir_47/$entry_47";
            if (-d $file_47) {
                $_find_47->($file_47, $depth_47 + 1);
            }
            elsif (-f $file_47) {
                next if !( basename($file_47) =~ m/^.*.sh$/xms );
                push @files_47, $file_47;
            }
        }
    };
    $_find_47->($start_47, 0);
    join "\n", @files_47;
};
print "Found shell scripts:\n";
print $found_files;
if ( !( ($found_files) =~ m{\n\z}msx ) ) { print "\n"; }
print "=== File and Directory Operations Complete ===\n";

exit $main_exit_code;

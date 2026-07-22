#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '041_process_substitution_mapfile.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== readarray/mapfile ==\n";
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
    my $output_250 = q{};
    my $output_printed_250;
    sprintf("x\ny\n");
if ($output_250 ne q{} && !$output_printed_250) {
    print $output_250;
}
}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_1} $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_1 or croak "Cannot open process substitution: $ERRNO\n";
my @lines = ();
if (open(my $mapfile_fh, '<', $temp_file_ps_fh_1)) {
    while (my $line = <$mapfile_fh>) {
        chomp $line;
        push @lines, $line;
    }
    close($mapfile_fh);
}
printf('%s ', (join(" ", @lines)));
print "\n";
$CHILD_ERROR = 0;

exit $main_exit_code;

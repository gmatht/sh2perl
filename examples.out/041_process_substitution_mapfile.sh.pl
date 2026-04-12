#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== readarray/mapfile ==\n";
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', "printf 'x\\ny\\n'");
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print $fh_ps_fh_1 $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
my @lines = ();
if (open(my $mapfile_fh, '<', $temp_file_ps_fh_1)) {
    while (my $line = <$mapfile_fh>) {
        chomp $line;
        push @lines, $line;
    }
    close($mapfile_fh);
}
foreach my $item (@lines) {
    printf("%s ", $item);
}
print "\n";

exit $main_exit_code;

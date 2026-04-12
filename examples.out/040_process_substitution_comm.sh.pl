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
print "== Process substitution with comm ==\n";
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', "printf 'a\\nb\\n'");
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
my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
my $output_ps_fh_2;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', "printf 'b\\nc\\n'");
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
}
use File::Path qw(make_path);
my $temp_dir_fh_2 = dirname($temp_file_ps_fh_2);
if (!-d $temp_dir_fh_2) { make_path($temp_dir_fh_2); }
open my $fh_ps_fh_2, '>', $temp_file_ps_fh_2 or croak "Cannot create temp file: $ERRNO\n";
print $fh_ps_fh_2 $output_ps_fh_2;
close $fh_ps_fh_2 or croak "Close failed: $ERRNO\n";
my @file1_lines;
my @file2_lines;
if (open(my $fh1, '<', $temp_file_ps_fh_1)) {
    while (my $line = <$fh1>) {
        chomp $line;
        push @file1_lines, $line;
    }
    close($fh1);
}
if (open(my $fh2, '<', $temp_file_ps_fh_2)) {
    while (my $line = <$fh2>) {
        chomp $line;
        push @file2_lines, $line;
    }
    close($fh2);
}
my %file1_set = map { $_ => 1 } @file1_lines;
my %file2_set = map { $_ => 1 } @file2_lines;
my @common_lines;
foreach my $line (@file1_lines) {
    if (exists($file2_set{$line})) {
        push @common_lines, $line;
    }
}
my $result = "";
$result .= join("\n", @common_lines) . "\n";
chomp $result;
print $result;
print "\n";

exit $main_exit_code;

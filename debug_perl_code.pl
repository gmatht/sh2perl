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

my $result;
$result = do { my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "file.txt") {
    open my $fh, '<', "file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: file.txt: No such file or directory\n"; }
my @grep_filtered_0 = grep { /pattern/msx } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z}msx || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
 $grep_result_0; };

exit $main_exit_code;

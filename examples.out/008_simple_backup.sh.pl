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

print "Hello, World!\n";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
{
    my $output_150 = q{};
    my $output_printed_150;
    my $pipeline_success_150 = 1;
        $output_150 = do {
    my @ls_files_151 = ();
    if ( -f q{.} ) {
    push @ls_files_151, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_151, $file;
    }
    closedir $dh;
    @ls_files_151 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_151;
    }
    }
    (@ls_files_151 ? join("\n", @ls_files_151) . "\n" : q{});
    };

        my $grep_result_150_1;
    my @grep_lines_150_1 = split /\n/msx, $output_150;
    my @grep_filtered_150_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_150_1;
    $grep_result_150_1 = join "\n", @grep_filtered_150_1;
    if (!($grep_result_150_1 =~ m{\n\z}msx || $grep_result_150_1 eq q{})) {
    $grep_result_150_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_150_1 > 0 ? 0 : 1;
    $output_150 = $grep_result_150_1;
    $output_150 = $grep_result_150_1;
    if ((scalar @grep_filtered_150_1) == 0) {
        $pipeline_success_150 = 0;
    }
    if ($output_150 ne q{} && !defined $output_printed_150) {
        print $output_150;
        if (!($output_150 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_150 ) { $main_exit_code = 1; }
    }
print join(" ", grep { length } split /\s+/msx, do { do {
    my $output_153 = q{};
    my $output_printed_153;
    my $pipeline_success_153 = 1;
    $output_153 = do {
    my @ls_files_154 = ();
    if ( -f q{.} ) {
    push @ls_files_154, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_154, $file;
    }
    closedir $dh;
    @ls_files_154 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_154;
    }
    }
    (@ls_files_154 ? join("\n", @ls_files_154) . "\n" : q{});
    };
    my $grep_result_153_1;
    my @grep_lines_153_1 = split /\n/msx, $output_153;
    my @grep_filtered_153_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_153_1;
    $grep_result_153_1 = join "\n", @grep_filtered_153_1;
    if (!($grep_result_153_1 =~ m{\n\z}msx || $grep_result_153_1 eq q{})) {
    $grep_result_153_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_153_1 > 0 ? 0 : 1;
    $output_153 = $grep_result_153_1;
    if ((scalar @grep_filtered_153_1) == 0) {
        $pipeline_success_153 = 0;
    }
    if ( !$pipeline_success_153 ) { $main_exit_code = 1; }
        $output_153 =~ s/\n+\z//msx;
    $output_153;
} });

exit $main_exit_code;

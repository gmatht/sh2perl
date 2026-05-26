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
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = do {
    my @ls_files_1 = ();
    if ( -f q{.} ) {
    push @ls_files_1, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_1, $file;
    }
    closedir $dh;
    @ls_files_1 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_1;
    }
    }
    (@ls_files_1 ? join("\n", @ls_files_1) . "\n" : q{});
    };

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
    if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
    $grep_result_0_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }
print join(" ", grep { length } split /\s+/msx, do { do {
    my $output_3 = q{};
    my $output_printed_3;
    my $pipeline_success_3 = 1;
    $output_3 = do {
    my @ls_files_4 = ();
    if ( -f q{.} ) {
    push @ls_files_4, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_4, $file;
    }
    closedir $dh;
    @ls_files_4 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_4;
    }
    }
    (@ls_files_4 ? join("\n", @ls_files_4) . "\n" : q{});
    };
    my $grep_result_3_1;
    my @grep_lines_3_1 = split /\n/msx, $output_3;
    my @grep_filtered_3_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_3_1;
    $grep_result_3_1 = join "\n", @grep_filtered_3_1;
    if (!($grep_result_3_1 =~ m{\n\z}msx || $grep_result_3_1 eq q{})) {
    $grep_result_3_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_3_1 > 0 ? 0 : 1;
    $output_3 = $grep_result_3_1;
    if ((scalar @grep_filtered_3_1) == 0) {
        $pipeline_success_3 = 0;
    }
    if ( !$pipeline_success_3 ) { $main_exit_code = 1; }
        $output_3 =~ s/\n+\z//msx;
    $output_3;
} });

exit $main_exit_code;

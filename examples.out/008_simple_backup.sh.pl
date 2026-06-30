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
    my $output_149 = q{};
    my $output_printed_149;
    my $pipeline_success_149 = 1;
        $output_149 = do {
    my @ls_files_150 = ();
    if ( -f q{.} ) {
    push @ls_files_150, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_150, $file;
    }
    closedir $dh;
    @ls_files_150 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_150;
    }
    }
    (@ls_files_150 ? join("\n", @ls_files_150) . "\n" : q{});
    };

        my $grep_result_149_1;
    my @grep_lines_149_1 = split /\n/msx, $output_149;
    my @grep_filtered_149_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_149_1;
    $grep_result_149_1 = join "\n", @grep_filtered_149_1;
    if (!($grep_result_149_1 =~ m{\n\z}msx || $grep_result_149_1 eq q{})) {
    $grep_result_149_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_149_1 > 0 ? 0 : 1;
    $output_149 = $grep_result_149_1;
    $output_149 = $grep_result_149_1;
    if ((scalar @grep_filtered_149_1) == 0) {
        $pipeline_success_149 = 0;
    }
    if ($output_149 ne q{} && !defined $output_printed_149) {
        print $output_149;
        if (!($output_149 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_149 ) { $main_exit_code = 1; }
    }
print join(" ", grep { length } split /\s+/msx, do { do {
    my $output_152 = q{};
    my $output_printed_152;
    my $pipeline_success_152 = 1;
    $output_152 = do {
    my @ls_files_153 = ();
    if ( -f q{.} ) {
    push @ls_files_153, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_153, $file;
    }
    closedir $dh;
    @ls_files_153 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_153;
    }
    }
    (@ls_files_153 ? join("\n", @ls_files_153) . "\n" : q{});
    };
    my $grep_result_152_1;
    my @grep_lines_152_1 = split /\n/msx, $output_152;
    my @grep_filtered_152_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_152_1;
    $grep_result_152_1 = join "\n", @grep_filtered_152_1;
    if (!($grep_result_152_1 =~ m{\n\z}msx || $grep_result_152_1 eq q{})) {
    $grep_result_152_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_152_1 > 0 ? 0 : 1;
    $output_152 = $grep_result_152_1;
    if ((scalar @grep_filtered_152_1) == 0) {
        $pipeline_success_152 = 0;
    }
    if ( !$pipeline_success_152 ) { $main_exit_code = 1; }
        if ($output_152 ne q{} && !($output_152 =~ m{\n\z}msx)) {
        $output_152 .= "\n";
    }
    $output_152;
} });

exit $main_exit_code;

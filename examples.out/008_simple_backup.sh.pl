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

$PROGRAM_NAME = '008_simple_backup.sh';
print "Hello, World!\n";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
{
    my $output_146 = q{};
    my $output_printed_146;
    my $pipeline_success_146 = 1;
        $output_146 = do {
    my @ls_files_147 = ();
    if ( -f q{.} ) {
    push @ls_files_147, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_147, $file;
    }
    closedir $dh;
    @ls_files_147 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_147;
    }
    }
    (@ls_files_147 ? join("\n", @ls_files_147) . "\n" : q{});
    };
    ;

        my $grep_result_146_1;
    my @grep_lines_146_1 = split /\n/msx, $output_146;
    my @grep_filtered_146_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_146_1;
    $grep_result_146_1 = join "\n", @grep_filtered_146_1;
    if (!($grep_result_146_1 =~ m{\n\z}msx || $grep_result_146_1 eq q{})) {
    $grep_result_146_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_146_1 > 0 ? 0 : 1;
    $output_146 = $grep_result_146_1;
    $output_146 = $grep_result_146_1;
    if ((scalar @grep_filtered_146_1) == 0) {
        $pipeline_success_146 = 0;
    }
    if ($output_146 ne q{} && !defined $output_printed_146) {
        print $output_146;
        if (!($output_146 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_146 ) { $main_exit_code = 1; }
    }
print join(" ", grep { length } split /\s+/msx, do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
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
    @ls_files_150 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_150;
    }
    }
    (@ls_files_150 ? join("\n", @ls_files_150) . "\n" : q{});
    };
    ;
    my $grep_result_149_1;
    my @grep_lines_149_1 = split /\n/msx, $output_149;
    my @grep_filtered_149_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_149_1;
    $grep_result_149_1 = join "\n", @grep_filtered_149_1;
    if (!($grep_result_149_1 =~ m{\n\z}msx || $grep_result_149_1 eq q{})) {
    $grep_result_149_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_149_1 > 0 ? 0 : 1;
    $output_149 = $grep_result_149_1;
    if ((scalar @grep_filtered_149_1) == 0) {
        $pipeline_success_149 = 0;
    }
    if ( !$pipeline_success_149 ) { $main_exit_code = 1; }
        $output_149 =~ s/\n+\z//msx;
    $output_149;
}; $_pipeline_result; });

exit $main_exit_code;

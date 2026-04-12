#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "Hello, World!\n";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
{
    my $output_157;
    my $output_printed_157;
    my $pipeline_success_157 = 1;
        $output_157 = do {
    my @ls_files_158 = ();
    if ( -f q{.} ) {
    push @ls_files_158, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_158, $file;
    }
    closedir $dh;
    @ls_files_158 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_158;
    }
    }
    (@ls_files_158 ? join("\n", @ls_files_158) . "\n" : q{});
    };

        my $grep_result_157_1;
    my @grep_lines_157_1 = split /\n/msx, $output_157;
    my @grep_filtered_157_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_157_1;
    $grep_result_157_1 = join "\n", @grep_filtered_157_1;
    if (!($grep_result_157_1 =~ m{\n\z}msx || $grep_result_157_1 eq q{})) {
    $grep_result_157_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_157_1 > 0 ? 0 : 1;
    $output_157 = $grep_result_157_1;
    $output_157 = $grep_result_157_1;
    if ((scalar @grep_filtered_157_1) == 0) {
        $pipeline_success_157 = 0;
    }
    if ($output_157 ne q{} && !defined $output_printed_157) {
        print $output_157;
        if (!($output_157 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_157 ) { $main_exit_code = 1; }
    }
print join(" ", grep { length } split /\s+/msx, do {
    my $output_160;
    my $pipeline_success_160 = 1;
    $output_160 = do {
    my @ls_files_161 = ();
    if ( -f q{.} ) {
    push @ls_files_161, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
    push @ls_files_161, $file;
    }
    closedir $dh;
    @ls_files_161 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_161;
    }
    }
    (@ls_files_161 ? join("\n", @ls_files_161) . "\n" : q{});
    };
    my $grep_result_160_1;
    my @grep_lines_160_1 = split /\n/msx, $output_160;
    my @grep_filtered_160_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_160_1;
    $grep_result_160_1 = join "\n", @grep_filtered_160_1;
    if (!($grep_result_160_1 =~ m{\n\z}msx || $grep_result_160_1 eq q{})) {
    $grep_result_160_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_160_1 > 0 ? 0 : 1;
    $output_160 = $grep_result_160_1;
    if ((scalar @grep_filtered_160_1) == 0) {
        $pipeline_success_160 = 0;
    }
    if ( !$pipeline_success_160 ) { $main_exit_code = 1; }
        $output_160;
});

exit $main_exit_code;

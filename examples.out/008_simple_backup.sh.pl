#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $ls_success     = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '008_simple_backup.sh';
say "Hello, World!";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
do {
    my $output_137 = q{};
    my $output_printed_137;
    my $pipeline_success_137 = 1;
        $output_137 = do {
    my @ls_files_138 = ();
    if ( -f q{.} ) {
    push @ls_files_138, q{.};
    }
    elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
    while ( my $file = readdir $dh ) {
    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
    push @ls_files_138, $file;
    }
    closedir $dh;
    @ls_files_138 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_files_138;
    }
    }
    (@ls_files_138 ? join("\n", @ls_files_138) . "\n" : q{});
    };
    ;

        my $grep_result_137_1;
    my @grep_lines_137_1 = split /\n/msx, $output_137;
    my @grep_filtered_137_1 = grep { !/__tmp_test_output.pl/msx } @grep_lines_137_1;
    $grep_result_137_1 = join "\n", @grep_filtered_137_1;
    if (!($grep_result_137_1 =~ m{\n\z} || $grep_result_137_1 eq q{})) {
    $grep_result_137_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_137_1 > 0 ? 0 : 1;
    $output_137 = $grep_result_137_1;
    $output_137 = $grep_result_137_1;
    if ((scalar @grep_filtered_137_1) == 0) {
        $pipeline_success_137 = 0;
    }
    if ($output_137 ne q{} && !defined $output_printed_137) {
        print $output_137;
        if (!($output_137 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_137 ) { $main_exit_code = 1; }
    }
print join(" ", grep { length } split /\s+/msx, do { chomp(my $result_140 = qx{ls | grep -v __tmp_test_output.pl}); $result_140; });

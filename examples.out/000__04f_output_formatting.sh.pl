#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04f_output_formatting.sh';
print "=== Output and Formatting Commands ===\n";
my $echo_result;
my @echo_result;
my %echo_result;
$echo_result = ("Hello from backticks");
do {
    my $output = "Echo result: $echo_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $printf_result;
my @printf_result;
my %printf_result;
$printf_result = sprintf("Number: %d, String: %s\n", '42', "test");
;
do {
    my $output = "Printf result: $printf_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $tee_result;
my @tee_result;
my %tee_result;
$tee_result = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_109 = q{};
    my $output_printed_109;
    my $pipeline_success_109 = 1;
    $output_109 .= 'test output' . "\n";
    if ( !($output_109 =~ m{\n\z}msx) ) { $output_109 .= "\n"; }
    $CHILD_ERROR = 0;
    if ($CHILD_ERROR != 0) { $pipeline_success_109 = 0; }
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee.txt' ) {
        print {$fh} $output_109;
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        carp "tee: Cannot open 'test_tee.txt': $ERRNO";
    }
    $output_109 = $output_109;
    if ( !$pipeline_success_109 ) { $main_exit_code = 1; }
    $output_109 =~ s/\n+\z//msx;
    $output_109;
}; $_pipeline_result; };
do {
    my $output = "Tee result: $tee_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
if ( -e "test_tee.txt" ) {
    if ( -d "test_tee.txt" ) {
        carp "rm: carping: ", "test_tee.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_tee.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "test_tee.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
print "=== Output and Formatting Commands Complete ===\n";

exit $main_exit_code;

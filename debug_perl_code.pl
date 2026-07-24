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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '087_function_cmd_sub.sh';

sub upper {
    my $val;
    $val = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $input_data = ("$_[0]") . "\n";
    my $set1_406 = 'a-z';
my $set2_406 = 'A-Z';
my $input_406 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_406 = $set1_406;
my $expanded_set2_406 = $set2_406;
# Handle a-z range in set1
if ($expanded_set1_406 =~ /a-z/msx) {
    $expanded_set1_406 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_406 =~ /A-Z/msx) {
    $expanded_set1_406 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_406 =~ /\[:upper:\]/msx) {
    $expanded_set1_406 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_406 =~ /\[:lower:\]/msx) {
    $expanded_set1_406 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_406 =~ /a-z/msx) {
    $expanded_set2_406 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_406 =~ /A-Z/msx) {
    $expanded_set2_406 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_406 =~ /\[:upper:\]/msx) {
    $expanded_set2_406 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_406 =~ /\[:lower:\]/msx) {
    $expanded_set2_406 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_405 = q{};
for my $char ( split //msx, $input_406 ) {
    my $pos_406 = index $expanded_set1_406, $char;
    if ( $pos_406 >= 0 && $pos_406 < length $expanded_set2_406 ) {
        $tr_result_405 .= substr $expanded_set2_406, $pos_406, 1;
    } else {
        $tr_result_405 .= $char;
    }
}
$tr_result_405
}; $_pipeline_result; };
    print $val;
if ( !( ($val) =~ m{\n\z}msx ) ) { print "\n"; }
    return;
}
upper("hello");
upper("world");

exit $main_exit_code;

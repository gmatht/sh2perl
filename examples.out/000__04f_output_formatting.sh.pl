#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "=== Output and Formatting Commands ===\n";
my $echo_result = do {
    my $_chomp_temp = ('Hello from backticks');
    chomp $_chomp_temp;
    $_chomp_temp;
};
do {
    my $output = "Echo result: $echo_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $printf_result = do {
    my $result = sprintf "Number: %d, String: %s\n", "42", "test";
    $result;
};
do {
    my $output = "Printf result: $printf_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $tee_result = do {
    my $output_113;
    my $pipeline_success_113 = 1;
    $output_113 .= "test output\n";
    if ( !($output_113 =~ m{\n\z}msx) ) { $output_113 .= "\n"; }
    my @lines = split /\n/msx, $output_113;
    if ( open my $fh, '>', 'test_tee.txt' ) {
        foreach my $line (@lines) {
            print {$fh} "$line\n";
        }
        close $fh
          or croak "Close failed: $ERRNO";
    }
    else {
        carp "tee: Cannot open 'test_tee.txt': $ERRNO";
    }
    if ( !$pipeline_success_113 ) { $main_exit_code = 1; }
    $output_113 =~ s/\n+\z//msx;
    $output_113;
};
do {
    my $output = "Tee result: $tee_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
if ( -e "test_tee.txt" ) {
    if ( -d "test_tee.txt" ) {
        carp "rm: carping: ", "test_tee.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_tee.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_tee.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
    carp "rm: carping: ", "test_tee.txt", ": No such file or directory\n";
}
print "=== Output and Formatting Commands Complete ===\n";

exit $main_exit_code;

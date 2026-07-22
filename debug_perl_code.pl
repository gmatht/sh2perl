#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = '063_11_complex_while_loop.sh';
my $line;
my @line;
my %line;

my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
    my $output_0 = q{};
    my $output_printed_0;
    my $head_line_count = 0;
    my $output_2 = q{};
    while (my $line = <>) {
        chomp $line;
            if (!($line =~ /^\#/msx)) {
            next;
        }
        if ($head_line_count < 10) {
        $output_2 .= $line . "\n";
        ++$head_line_count;
    } else {
        $line = q{}; # Clear line to prevent printing
        last; # Break out of the yes loop when head limit is reached
    }
        print $line . "\n";
    }
    $output_2;
}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_1} $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_1 or croak "Cannot open process substitution: $ERRNO\n";
while (1) {
    my $IFS = q{};
    last unless $CHILD_ERROR == 0;
    last unless do {
        my $L = <>;
        chomp $L;
        $CHILD_ERROR == 0
    };
    last unless ("$line" ne q{});
    last unless do {
        $main_exit_code = eval { int($ENV{counter} < $ENV{max_lines}) } // "";
        $CHILD_ERROR == 0
    };
if ("$line" =~ /^[[:space:]]*\#/msx) {
next;
    }
if ("$line" =~ /^.*\$\(.*\).*$/msx) {
                do {
    my $output = "Contains command substitution: $line";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
        $CHILD_ERROR = 0;
    } elsif ("$line" =~ /^.*\$\{\[^}\].*\}.*$/msx) {
                do {
    my $output = "Contains parameter expansion: $line";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
        $CHILD_ERROR = 0;
    } elsif ("$line" =~ /^.*\$\(\(.*\)\).*$/msx) {
                do {
    my $output = "Contains arithmetic expansion: $line";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
        $CHILD_ERROR = 0;
    }
    $main_exit_code = eval { int($ENV{counter}++) } // "";
}

exit $main_exit_code;

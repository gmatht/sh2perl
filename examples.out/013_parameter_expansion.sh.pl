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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '013_parameter_expansion.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Case modification in parameter expansion ==\n";
my $name;
my @name;
my %name;
$name = "world";
do {
    my $__echo_line = uc(${name});
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = lc(${name});
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = ucfirst(${name});
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
print "== Advanced parameter expansion ==\n";
my $path;
my @path;
my %path;
$path = "/tmp/file.txt";
do {
    my $__echo_line = basename(${path});
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = dirname(${path});
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
my $s2;
my @s2;
my %s2;
$s2 = "abba";
print $s2 =~ s/b/X/grs;
if ( !( ($s2 =~ s/b/X/grs) =~ m{\n\z}msx ) ) { print "\n"; }
print "== More parameter expansion ==\n";
my $var;
my @var;
my %var;
$var = "hello world";
print ${var} =~ s/^hello//r;
if ( !( (${var} =~ s/^hello//r) =~ m{\n\z}msx ) ) { print "\n"; }
do {
    my $__echo_line = scalar reverse( (scalar reverse ${var}) =~ s/^dlrow//r );
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
print $var =~ s/o/0/grs;
if ( !( ($var =~ s/o/0/grs) =~ m{\n\z}msx ) ) { print "\n"; }
print "== Default values ==\n";
delete $ENV{maybe};
do {
    my $__echo_line = (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : 'default');
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : do { $ENV{maybe} = 'default'; ($ENV{maybe} // q{}) });
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : die('error'));
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;

exit $main_exit_code;

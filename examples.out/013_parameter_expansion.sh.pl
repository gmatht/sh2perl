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

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Case modification in parameter expansion ==\n";
my $name = "world";
do {
    my $output = uc(${name});
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = lc(${name});
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = ucfirst(${name});
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
print "== Advanced parameter expansion ==\n";
my $path = "/tmp/file.txt";
do {
    my $output = basename(${path});
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = dirname(${path});
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $s2;
$s2 = "abba";
print $s2 =~ s/b/X/grs;
if ( !( ($s2 =~ s/b/X/grs) =~ m{\n\z}msx ) ) { print "\n"; }
print "== More parameter expansion ==\n";
my $var = "hello world";
print ${var} =~ s/^hello//r;
if ( !( (${var} =~ s/^hello//r) =~ m{\n\z}msx ) ) { print "\n"; }
do {
    my $output = scalar reverse( (scalar reverse ${var}) =~ s/^dlrow//r );
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
print $var =~ s/o/0/grs;
if ( !( ($var =~ s/o/0/grs) =~ m{\n\z}msx ) ) { print "\n"; }
print "== Default values ==\n";
my $maybe;
undef $maybe;
delete $ENV{maybe};
do {
    my $output = defined ${maybe} && ${maybe} ne q{} ? ${maybe} : 'default';
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = defined ${maybe} && ${maybe} ne q{} ? ${maybe} : do { ${maybe} = 'default'; ${maybe} };
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = defined ${maybe} && ${maybe} ne q{} ? ${maybe} : die('error');
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;

exit $main_exit_code;

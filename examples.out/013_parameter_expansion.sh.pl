#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Basename;
my $__set_e = 0;
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Case modification in parameter expansion ==\n";
my $name = "world";
print uc(${name}), "\n";
print lc(${name}), "\n";
print ucfirst(${name}), "\n";
print "== Advanced parameter expansion ==\n";
my $path = "/tmp/013_param_expansion_file.txt";
print basename(${path}), "\n";
print dirname(${path}), "\n";
my $s2 = "abba";
print $s2 =~ s/b/X/grs, "\n";
print "== More parameter expansion ==\n";
my $var = "hello world";
print ${var} =~ s/^hello//r, "\n";
print scalar reverse( (scalar reverse ${var}) =~ s/^dlrow//r ), "\n";
print $var =~ s/o/0/grs, "\n";
print "== Default values ==\n";
delete $ENV{maybe};
print (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : 'default'), "\n";
print (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : do { $ENV{maybe} = 'default'; ($ENV{maybe} // q{}) }), "\n";
print (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : die('error')), "\n";


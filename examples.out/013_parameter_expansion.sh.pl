#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use File::Basename;
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '013_parameter_expansion.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== Case modification in parameter expansion ==";
my $name = "world";
say uc(${name});
say lc(${name});
say ucfirst(${name});
say "== Advanced parameter expansion ==";
my $path = "/tmp/013_param_expansion_file.txt";
say basename(${path});
say dirname(${path});
my $s2 = "abba";
say $s2 =~ s/b/X/grs;
say "== More parameter expansion ==";
my $var = "hello world";
say ${var} =~ s/^hello//r;
say scalar reverse( (scalar reverse ${var}) =~ s/^dlrow//r );
say $var =~ s/o/0/grs;
say "== Default values ==";
delete $ENV{maybe};
say (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : 'default');
say (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : do { $ENV{maybe} = 'default'; ($ENV{maybe} // q{}) });
say (defined ($ENV{maybe} // q{}) && ($ENV{maybe} // q{}) ne q{} ? ($ENV{maybe} // q{}) : die('error'));

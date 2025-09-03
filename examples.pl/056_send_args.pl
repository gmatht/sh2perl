#!/usr/bin/env perl

use strict;
use warnings;

# Send arguments to the args script
system("perl examples.pl/005_args.pl one");
system("perl examples.pl/005_args.pl one two");
system("perl examples.pl/005_args.pl one two three");
system("perl examples.pl/005_args.pl 1");
system("perl examples.pl/005_args.pl 1 2 3");
system("perl examples.pl/005_args.pl 1 two 3");
system("perl examples.pl/005_args.pl \"A 'quoted' Sting\"");
system("perl examples.pl/005_args.pl \"A 'quoted' Sting\" 2 3 4 5 6");

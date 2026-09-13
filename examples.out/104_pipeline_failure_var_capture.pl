#!/usr/bin/env perl
use strict;
use warnings;
my $files = do { local $CHILD_ERROR = 0; q{}; };
say "Files in /tmp: $files";
my $count = do { local $CHILD_ERROR = 0; q{}; };
say "Words: $count";
say "done";



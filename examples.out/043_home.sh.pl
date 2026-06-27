#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
my $pipeline_result_1 = (($ENV{'HOME'} eq $ENV{'HOME'})) && (print("1\n")) || (print("\n"));
my $pipeline_result_2 = (($ENV{'HOME'} . '/Documents') eq $ENV{'HOME'}) && (print("2\n")) || (print("\n"));
my $pipeline_result_3 = (($ENV{'HOME'} . '/Documents') eq ($ENV{'HOME'} . '/Documents')) && (print("3\n")) || (print("\n"));
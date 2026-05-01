#!/usr/bin/perl

# Example 020: Disabled — use the system diff utility

use strict;
use warnings;

print "=== Example 020: diff example disabled ===\n";

print <<'NOTE';
This example is intentionally disabled. Implementing a byte-for-byte compatible
replacement for the system 'diff' utility in a short Perl example is non-trivial.

To compare files use the system diff utility. Example:

  printf '%s\n' "a\nb\nc" > a.txt
  printf '%s\n' "a\nb modified\nc" > b.txt
  diff -u a.txt b.txt

NOTE

print "=== Example 020 completed (disabled) ===\n";

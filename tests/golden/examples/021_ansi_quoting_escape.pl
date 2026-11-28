#!/usr/bin/env perl
use strict;
use warnings;

$? = 0 << 8;
print "== Escape sequences ==\n";
$? = 0 << 8;
$? = 0 << 8;
print "bell\n";
$? = 0 << 8;
$? = 0 << 8;
print "backspace\n";
$? = 0 << 8;
$? = 0 << 8;
print "formfeed\n";
$? = 0 << 8;
$? = 0 << 8;
print "newline\n\n";
$? = 0 << 8;
$? = 0 << 8;
print "carriage\rreturn\n";
$? = 0 << 8;
$? = 0 << 8;
print "tab\tseparated\n";
$? = 0 << 8;
$? = 0 << 8;
print "verticaltab\n";
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;

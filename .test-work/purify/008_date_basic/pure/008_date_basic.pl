#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/008_date_basic.pl" }


use strict;
use warnings;
use POSIX qw(strftime);

print "=== Example 008: Basic date command ===\n";

my $epoch = 1672576496; 
my $date_output = strftime("%a %b %e %H:%M:%S UTC %Y", gmtime($epoch));

print "Using a fixed timestamp:\n";
print "$date_output\n";

print "\ndate with specific format:\n";
print strftime("%Y-%m-%d %H:%M:%S", gmtime($epoch)), "\n";

print "\ndate with different formats:\n";
my $date_iso = strftime("%Y-%m-%d", gmtime($epoch));
print "ISO date: $date_iso\n";

my $date_time = strftime("%H:%M:%S", gmtime($epoch));
print "Time: $date_time\n";

my $date_weekday = strftime("%A", gmtime($epoch));
print "Weekday: $date_weekday\n";

print "\ndate with custom format:\n";
print "Today is ", strftime("%A, %B %d, %Y", gmtime($epoch)), "\n";

print "\ndate with timezone:\n";
print "Timezone: UTC\n";

print "\ndate with epoch time:\n";
print "$epoch\n";

print "\ndate with readable epoch time:\n";
print "Epoch $epoch = $date_output\n";

print "\ndate with file modification time:\n";
open(my $date_fh, '>', 'date_reference.txt') or die "Cannot create file: $!\n";
close($date_fh);
utime $epoch, $epoch, 'date_reference.txt' or die "Cannot set file time: $!\n";
my $file_mtime = (stat('date_reference.txt'))[9];
print strftime("%a %b %e %H:%M:%S UTC %Y", gmtime($file_mtime)), "\n";

print "\ndate with different locales:\n";
{
    local $ENV{LC_TIME} = 'C';
    my $date_locale = strftime("%a %b %e %H:%M:%S UTC %Y", gmtime($epoch));
    print "C locale: $date_locale\n";
}

unlink('date_reference.txt') if -f 'date_reference.txt';

print "=== Example 008 completed successfully ===\n";

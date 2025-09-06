#!/usr/bin/perl

# Example 008: Basic date command using system() and backticks
# This demonstrates the date builtin called from Perl

print "=== Example 008: Basic date command ===\n";

# Simple date command using backticks
print "Using backticks to call date:\n";
my $date_output = `date`;
print $date_output;

# date with specific format using system()
print "\ndate with specific format:\n";
system("date", "+%Y-%m-%d %H:%M:%S");

# date with different formats using backticks
print "\ndate with different formats:\n";
my $date_iso = `date +%Y-%m-%d`;
print "ISO date: $date_iso";

my $date_time = `date +%H:%M:%S`;
print "Time: $date_time";

my $date_weekday = `date +%A`;
print "Weekday: $date_weekday";

# date with custom format using system()
print "\ndate with custom format:\n";
system("date", "+Today is %A, %B %d, %Y");

# date with timezone using backticks
print "\ndate with timezone:\n";
my $date_tz = `date +%Z`;
print "Timezone: $date_tz";

# date with epoch time using system()
print "\ndate with epoch time:\n";
system("date", "+%s");

# date with readable epoch time using backticks
print "\ndate with readable epoch time:\n";
my $epoch = `date +%s`;
chomp $epoch;
my $readable = `date -d @$epoch`;
print "Epoch $epoch = $readable";

# date with file modification time using system()
print "\ndate with file modification time:\n";
system("date", "-r", "README.md") if -f "README.md";

# date with different locales using backticks
print "\ndate with different locales:\n";
my $date_locale = `LC_TIME=C date`;
print "C locale: $date_locale";

print "=== Example 008 completed successfully ===\n";

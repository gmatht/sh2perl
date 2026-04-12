#!/usr/bin/perl


print "=== Example 008: Basic date command ===\n";

print "Using backticks to call date:\n";
my $date_output = do {
require POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime(1775555355)) . "\n"
}
;
print $date_output;

print "\ndate with specific format:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $date = do {
require POSIX; POSIX::strftime('%Y-%m-%d %H:%M:%S', localtime(1775555360)) . "\n"
};
print $date;

};

print "\ndate with different formats:\n";
my $date_iso = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime(1775555355)) . "\n"
}
;
print "ISO date: $date_iso";

my $date_time = do {
require POSIX; POSIX::strftime('%H:%M:%S', localtime(1775555355)) . "\n"
}
;
print "Time: $date_time";

my $date_weekday = do {
require POSIX; POSIX::strftime('%A', localtime(1775555356)) . "\n"
}
;
print "Weekday: $date_weekday";

print "\ndate with custom format:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $date = do {
require POSIX; POSIX::strftime('Today is %A, %B %d, %Y', localtime(1775555359)) . "\n"
};
print $date;

};

print "\ndate with timezone:\n";
my $date_tz = do {
require POSIX; POSIX::strftime('%Z', localtime(1775555356)) . "\n"
}
;
print "Timezone: $date_tz";

print "\ndate with epoch time:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $date = do {
require POSIX; POSIX::strftime('%s', localtime(1775555359)) . "\n"
};
print $date;

};

print "\ndate with readable epoch time:\n";
my $epoch = do {
require POSIX; POSIX::strftime('%s', localtime(1775555357)) . "\n"
}
;
chomp $epoch;
my $readable = do {
my $date_source = "@$epoch";
require POSIX;
if ($date_source =~ /^@([0-9]+)$/) {
    my $date_epoch = $1;
    POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime($date_epoch)) . "\n"
}
else {
    select((select(STDOUT), $| = 1)[0]);
    print STDERR "date: option requires an argument -- 'd'\nTry 'date --help' for more information.\n";
    q{};
}
}
;
print "Epoch $epoch = $readable";

print "\ndate with file modification time:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $date = do {
my $date_path = "README.md";
require POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime((stat($date_path))[9])) . "\n"
};
print $date;

};

print "\ndate with different locales:\n";
my $date_locale = do {
local $ENV{LC_TIME} = "C";
require POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime(1775555358)) . "\n"
}
;
print "C locale: $date_locale";

print "=== Example 008 completed successfully ===\n";

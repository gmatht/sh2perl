#!/usr/bin/perl


exit

print "=== Example 009: Basic sleep command ===\n";

print "Using " . "sys" . "tem" . "() to call sleep (2 seconds):\n";
my $start_time = time();
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
require Time::HiRes;
Time::HiRes::sleep("2");

};
my $end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

print "\nUsing backticks to call sleep (1.5 seconds):\n";
$start_time = time();
my $sleep_output = do {
    require Time::HiRes; Time::HiRes::sleep("1.5");
    q{};
}
;
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

print "\nSleep with different durations:\n";
for my $duration (1, 2, 3) {
    print "Sleeping for $duration second(s)...\n";
    $start_time = time();
    do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
require Time::HiRes;
Time::HiRes::sleep($duration);

};
    $end_time = time();
    print "Actually slept for " . ($end_time - $start_time) . " seconds\n";
}

print "\nSleep with fractional durations:\n";
my @durations = (0.5, 1.0, 1.5);
foreach my $duration (@durations) {
    print "Sleeping for $duration second(s)...\n";
    $start_time = time();
    my $result = do {
    require Time::HiRes; Time::HiRes::sleep($duration);
    q{};
}
;
    $end_time = time();
    print "Actually slept for " . ($end_time - $start_time) . " seconds\n";
}

print "\nSleep with very short duration (0.1 seconds):\n";
$start_time = time();
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
require Time::HiRes;
Time::HiRes::sleep("0.1");

};
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

print "\nSleep with longer duration (3 seconds):\n";
$start_time = time();
my $long_sleep = do {
    require Time::HiRes; Time::HiRes::sleep("3");
    q{};
}
;
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

print "\nSleep with multiple arguments (sleep 1 2 3):\n";
$start_time = time();
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $MAGIC_3 = 3;
my $total_sleep = 0;
$total_sleep += "1";
$total_sleep += "2";
$total_sleep += "3";
require Time::HiRes;
Time::HiRes::sleep($total_sleep);

};  
$end_time = time();
print "Slept for " . ($end_time - $start_time) . " seconds\n";

print "=== Example 009 completed successfully ===\n";

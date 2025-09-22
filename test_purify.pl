#!/usr/bin/env perl
use strict;
use warnings;
use Getopt::Long;

# Command line options
my $verbose = 1;
my $next = 0;
my $purify_tested=0;
my $purify_passed=0;
my $purify_failed=0;

GetOptions(
	#'verbose|v' => \$verbose,
    'next' => \$next,
) or die "Error in command line arguments\n";

if ($verbose) {
    print "Running test_purify.pl with verbose output\n";
}

if ($next) {
    print "Running test_purify.pl with next option\n";
}

# Test purify.pl on all files from examples.impurl
my @test_files = glob("examples.impurl/*.pl");

if ($verbose) {
    print "Found " . scalar(@test_files) . " .pl files in examples.impurl directory\n";
}

my $tested_count = 0;
my $skipped_count = 0;

# Test that purify.pl can handle the --help option
if ($verbose) {
    print "Testing purify.pl --help...\n";
}

my $help_output = `perl purify.pl --help 2>&1`;
my $help_result = $? >> 8;
if ($help_result != 0) {
    print "Error: purify.pl --help failed (exit code: $help_result)\n";
    print "Error output:\n";
    print join("\n", split(/\n/, $help_output)) . "\n";
    die "Stopping on first failure. Fix the issue and run again.\n";
}



foreach my $perl_file (@test_files) {
    if (-f $perl_file) {
	my $pure_file="pure/" . `basename $perl_file`;
        if ($verbose) {
            print "  Testing purify.pl on $perl_file -> $pure_file...\n";
        }
        
        # Test purify.pl on the Perl file and capture output
        my $output = `perl purify.pl "$perl_file" | tee $pure_file`; 
        my $purify_result = $? >> 8;
        $purify_tested++;
        
        if ($purify_result == 0) {
            if ($verbose) {
                print "    ✓ $perl_file: purify.pl processed successfully\n";
            }

	    if ( system("grep -e '(system|`)' $pure_file" ) ){
		    print "Failed to Purify $pure_file";
		    exit;
            }

	    my $out1 = `perl $perl_file | tee out1.txt`;
	    my $out2 = `perl $pure_file | tee out2.txt`;

	    if ( $out1 ne $out2 ) {
		    print " === purified === \n$output\n === end purified ===";

		    system("diff -u out1.txt out2.txt");
		    print "FAILED\n";
		    exit;
	    }
	    $purify_passed++;
        } else {
            print "    ✗ $perl_file: purify.pl failed (exit code: $purify_result)\n";
            print "    Error output:\n";
            print "    " . join("\n    ", split(/\n/, $output)) . "\n";
            $purify_failed++;
            # Quit on first failure
            die "Stopping on first failure. Fix the issue and run again.\n";
        }
    }
}

if ($verbose) {
    print "  Purify.pl test summary: $purify_passed passed, $purify_failed failed out of $purify_tested tested\n";
}

# If any purify tests failed, this is a critical error
if ($purify_failed > 0) {
    die "Error: $purify_failed purify.pl tests failed. The purify.pl script is not working correctly.\n";
}

if ($verbose) {
    print "test_purify.pl completed successfully\n";
    print "Summary: Tested $tested_count files, skipped $skipped_count files\n";
    print "Purify.pl tests: $purify_passed passed, $purify_failed failed out of $purify_tested tested\n";
}

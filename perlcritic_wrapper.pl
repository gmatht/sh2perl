#!/usr/bin/perl
use strict;
use warnings;
use Perl::Critic;
use Perl::Tidy;

# Parse command line arguments
my $profile_file;
my $file;
for my $i (0..$#ARGV) {
    if ($ARGV[$i] eq '--profile' && $i < $#ARGV) {
        $profile_file = $ARGV[$i + 1];
    } elsif ($ARGV[$i] !~ /^--/) {
        $file = $ARGV[$i];
    }
}

# Create critic with profile if specified
my $critic;
if ($profile_file) {
    $critic = Perl::Critic->new(
        -profile => $profile_file,
    );
} else {
    $critic = Perl::Critic->new(
        -severity => 1,  # brutal
    );
}

my @violations = $critic->critique($file);

# Check for "Code is not tidy" violations and replace with perltidy check
my @filtered_violations;
my $tidy_check_failed = 0;

foreach my $violation (@violations) {
    if ($violation->policy() eq 'Perl::Critic::Policy::CodeLayout::RequireTidyCode') {
        # Replace with perltidy check
        if (check_tidy($file)) {
            $tidy_check_failed = 1;
        }
    } else {
        push @filtered_violations, $violation;
    }
}

if ($tidy_check_failed) {
    print "FAILED: Code is not tidy\n";
}

if (@filtered_violations) {
    print "Perl::Critic violations found:\n";
    foreach my $violation (@filtered_violations) {
        print $violation->description() . "\n";
        print "  Policy: " . $violation->policy() . "\n";
        print "  Severity: " . $violation->severity() . "\n";
        print "  Location: " . $violation->location() . "\n";
        print "  Explanation: " . $violation->explanation() . "\n";
        print "\n";
    }
}

if ($tidy_check_failed || @filtered_violations) {
    exit 1;
} else {
    print "No violations found.\n";
    exit 0;
}

sub check_tidy {
    my $file = shift;
    
    # Read the original file
    open my $fh, '<', $file or die "Cannot open $file: $!";
    my $original = do { local $/; <$fh> };
    close $fh;
    
    # Use the working minimal wrapper approach by calling it as a separate process
    my $tidy_output = `C:/Strawberry/perl/bin/perl.exe test_wrapper_minimal.pl $file`;
    
    if ($? != 0) {
        # If perltidy failed, fall back to the original violation
        print "perltidy failed with exit code: $?\n";
        return 1;
    }
    
    # Extract the tidied output from the wrapper output
    my @lines = split /\n/, $tidy_output;
    my $tidy_start = 0;
    my $tidy_content = "";
    
    foreach my $line (@lines) {
        if ($line eq "Tidied:") {
            $tidy_start = 1;
            next;
        }
        if ($tidy_start) {
            $tidy_content .= $line . "\n";
        }
    }
    
    # Compare original with tidied version
    if ($original ne $tidy_content) {
        print "Code formatting differences detected:\n";
        print "Original:\n$original\n";
        print "Tidied:\n$tidy_content\n";
        return 1;
    }
    
    return 0;
}

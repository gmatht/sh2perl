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
        # Skip location information since it's not working properly
        # my $location = $violation->location();
        # print "  Location: " . $location . "\n";
        print "  Location: " . join(", ", @{$violation->location()}) . "\n";
        print "  Explanation: " . $violation->explanation() . "\n";
        
        # Add specific guidance for common violations
        my $policy = $violation->policy();
        if ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitConstantPragma') {
            print "  How to fix: Replace 'use constant NAME => VALUE;' with 'my \$NAME = VALUE;' or define constants differently\n";
    } elsif ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitInterpolationOfLiterals') {
        print "  How to fix: Perl::Critic sees double quotes and thinks interpolation might be happening. Use single quotes for literal strings that don't contain variables (e.g., 'Hello, World!\\n' instead of \"Hello, World!\\n\"). The \\n is just a literal escape sequence, not variable interpolation.\n";
        } elsif ($policy eq 'Perl::Critic::Policy::CodeLayout::ProhibitParensWithBuiltins') {
            print "  How to fix: Remove unnecessary parentheses around built-in functions like 'print()' -> 'print'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitNoisyQuotes') {
            print "  How to fix: Use single quotes for strings that don't need interpolation, like 'text' instead of \"text\"\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ControlStructures::ProhibitPostfixControls') {
            print "  How to fix: Use block form instead of postfix, like 'unless (condition) { ... }' instead of 'statement unless condition'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::RequireDotMatchAnything') {
            print "  How to fix: Add /s flag to regex to make . match newlines: s/pattern/replacement/s\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::RequireExtendedFormatting') {
            print "  How to fix: Add /x flag to regex for better readability: s/pattern/replacement/x\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::RequireLineBoundaryMatching') {
            print "  How to fix: Add /m flag to regex for multiline matching: s/pattern/replacement/m\n";
        } elsif ($policy eq 'Perl::Critic::Policy::InputOutput::ProhibitBacktickOperators') {
            print "  How to fix: Use IPC::Open3 or system() instead of backticks for better security and error handling\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitEmptyQuotes') {
            print "  How to fix: Use q{} or qw{} instead of empty quotes, or remove unnecessary empty strings\n";
        } elsif ($policy eq 'Perl::Critic::Policy::InputOutput::RequireCheckedClose') {
            print "  How to fix: Check the return value of close(): 'close(\$fh) or die \"Close failed: \$!\";'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ErrorHandling::RequireCarping') {
            print "  How to fix: Use 'carp' or 'croak' from Carp module instead of 'warn' or 'die'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::Subroutines::RequireFinalReturn') {
            print "  How to fix: Add explicit 'return;' at the end of subroutines\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::ProhibitEscapedMetacharacters') {
            print "  How to fix: Use character classes instead of escaping, like [.] instead of \\.\n";
        } elsif ($policy eq 'Perl::Critic::Policy::References::ProhibitDoubleSigils') {
            print "  How to fix: Use proper dereferencing syntax, like \\\$\\\$ref instead of \\\$\\\$\\\$ref\n";
        }
        
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

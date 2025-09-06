#!/usr/bin/perl
use strict;
use warnings;
use Perl::Critic;
use Perl::Tidy;
use Digest::SHA qw(sha1_hex);
use File::Spec;
use File::stat;
use Time::HiRes qw(gettimeofday tv_interval);

# Start timing
my $start_time = [gettimeofday];

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

# Log start time
my $parse_time = tv_interval($start_time);
print STDERR "perlcritic_wrapper: Started, parse args took ${parse_time}s\n";

# Cache directory
my $cache_dir = "cache/perlcritic";

# Check if we should use cache
my $use_cache = 1;
my $cache_file;
my $perl_code;

if ($file && -f $file) {
    my $file_start = [gettimeofday];
    
    # Read the Perl code
    open my $fh, '<', $file or die "Cannot open $file: $!";
    $perl_code = do { local $/; <$fh> };
    close $fh;
    
    my $file_read_time = tv_interval($file_start);
    print STDERR "perlcritic_wrapper: File read took ${file_read_time}s\n";
    
    # Generate SHA1 hash of the Perl code
    my $hash_start = [gettimeofday];
    my $sha1 = sha1_hex($perl_code);
    $cache_file = File::Spec->catfile($cache_dir, $sha1);
    my $hash_time = tv_interval($hash_start);
    print STDERR "perlcritic_wrapper: SHA1 hash took ${hash_time}s\n";
    
    # Check if cache directory exists, create if not
    unless (-d $cache_dir) {
        mkdir -p $cache_dir or die "Cannot create cache directory $cache_dir: $!";
    }
    
    # Check if cache file exists and is newer than perlcritic.conf
    if (-f $cache_file) {
        my $cache_check_start = [gettimeofday];
        my $cache_stat = stat($cache_file);
        my $config_file = $profile_file || "docs/perlcritic.conf";
        
        if (-f $config_file) {
            my $config_stat = stat($config_file);
            if ($cache_stat->mtime >= $config_stat->mtime) {
                # Cache is valid, read and output cached result
                open my $cache_fh, '<', $cache_file or die "Cannot open cache file $cache_file: $!";
                my $cached_output = do { local $/; <$cache_fh> };
                close $cache_fh;
                my $cache_time = tv_interval($cache_check_start);
                print STDERR "perlcritic_wrapper: Cache hit, read took ${cache_time}s\n";
                print $cached_output;
                exit ($cached_output =~ /No violations found/ ? 0 : 1);
            }
        } else {
            # No config file, cache is valid
            open my $cache_fh, '<', $cache_file or die "Cannot open cache file $cache_file: $!";
            my $cached_output = do { local $/; <$cache_fh> };
            close $cache_fh;
            my $cache_time = tv_interval($cache_check_start);
            print STDERR "perlcritic_wrapper: Cache hit (no config), read took ${cache_time}s\n";
            print $cached_output;
            exit ($cached_output =~ /No violations found/ ? 0 : 1);
        }
        my $cache_check_time = tv_interval($cache_check_start);
        print STDERR "perlcritic_wrapper: Cache miss, check took ${cache_check_time}s\n";
    } else {
        print STDERR "perlcritic_wrapper: No cache file found\n";
    }
}

# Create critic with profile if specified
my $critic_start = [gettimeofday];
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
my $critic_init_time = tv_interval($critic_start);
print STDERR "perlcritic_wrapper: Perl::Critic init took ${critic_init_time}s\n";

# Run Perl::Critic analysis
my $analysis_start = [gettimeofday];
my @violations = $critic->critique($file);
my $analysis_time = tv_interval($analysis_start);
print STDERR "perlcritic_wrapper: Perl::Critic analysis took ${analysis_time}s\n";

# Check for "Code is not tidy" violations and replace with perltidy check
my $tidy_start = [gettimeofday];
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
my $tidy_time = tv_interval($tidy_start);
print STDERR "perlcritic_wrapper: Perltidy check took ${tidy_time}s\n";


# Capture output for caching
my $output_start = [gettimeofday];
my $output = "";
if ($tidy_check_failed) {
    $output .= "FAILED: Code is not tidy\n";
}

if (@filtered_violations) {
    $output .= "Perl::Critic violations found:\n";
    foreach my $violation (@filtered_violations) {
        $output .= $violation->description() . "\n";
        $output .= "  Policy: " . $violation->policy() . "\n";
        $output .= "  Severity: " . $violation->severity() . "\n";
        # Get location information and show the actual line with violation marker
        my $location = $violation->location();
        my @location_parts = @{$location};
        if (@location_parts >= 2) {
            my $line_num = $location_parts[0];
            my $column = $location_parts[1];
            $output .= "  Location: Line $line_num, Column $column\n";
            
            # Read the file and show the line with violation marker
            # Find the actual Perl file by looking for .pl files in ARGV
            my $perl_file = '__tmp_test_output.pl';
            foreach my $arg (@ARGV) {
                if ($arg =~ /\.pl$/) {
                    $perl_file = $arg;
                    last;
                }
            }
            if (open(my $fh, '<', $perl_file)) {
                my $current_line = 0;
                while (my $line = <$fh>) {
                    $current_line++;
                    if ($current_line == $line_num) {
                        chomp $line;
                        # Insert [VIOLATION!] at the column position
                        my $before = substr($line, 0, $column - 1);
                        my $after = substr($line, $column - 1);
                        $output .= "  Code: " . $before . "[VIOLATION!]" . $after . "\n";
                        last;
                    }
                }
                close($fh);
            } else {
                $output .= "  Debug: Could not open file: $perl_file - $!\n";
            }
        } else {
            $output .= "  Location: " . join(", ", @location_parts) . "\n";
        }
        $output .= "  Explanation: " . $violation->explanation() . "\n";
        
        # Add specific guidance for common violations
        my $policy = $violation->policy();
        if ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitConstantPragma') {
            $output .= "  How to fix: Replace 'use constant NAME => VALUE;' with 'my \$NAME = VALUE;' or define constants differently\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitInterpolationOfLiterals') {
            $output .= "  How to fix: Perl::Critic sees double quotes and thinks interpolation might be happening. Use single quotes for literal strings that don't contain variables (e.g., 'Hello, World!\\n' instead of \"Hello, World!\\n\"). The \\n is just a literal escape sequence, not variable interpolation.\n";
        } elsif ($policy eq 'Perl::Critic::Policy::CodeLayout::ProhibitParensWithBuiltins') {
            $output .= "  How to fix: Remove unnecessary parentheses around built-in functions like 'print()' -> 'print'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitNoisyQuotes') {
            $output .= "  How to fix: Use single quotes for strings that don't need interpolation, like 'text' instead of \"text\"\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ControlStructures::ProhibitPostfixControls') {
            $output .= "  How to fix: Use block form instead of postfix, like 'unless (condition) { ... }' instead of 'statement unless condition'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::RequireDotMatchAnything') {
            $output .= "  How to fix: Add /s flag to regex to make . match newlines: s/pattern/replacement/s\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::RequireExtendedFormatting') {
            $output .= "  How to fix: Add /x flag to regex for better readability: s/pattern/replacement/x\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::RequireLineBoundaryMatching') {
            $output .= "  How to fix: Add /m flag to regex for multiline matching: s/pattern/replacement/m\n";
        } elsif ($policy eq 'Perl::Critic::Policy::InputOutput::ProhibitBacktickOperators') {
            $output .= "  How to fix: Use IPC::Open3 or system() instead of backticks for better security and error handling\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ValuesAndExpressions::ProhibitEmptyQuotes') {
            $output .= "  How to fix: Use q{} or qw{} instead of empty quotes, or remove unnecessary empty strings\n";
        } elsif ($policy eq 'Perl::Critic::Policy::InputOutput::RequireCheckedClose') {
            $output .= "  How to fix: Check the return value of close(): 'close(\$fh) or die \"Close failed: \$!\";'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::ErrorHandling::RequireCarping') {
            $output .= "  How to fix: Use 'carp' or 'croak' from Carp module instead of 'warn' or 'die'\n";
        } elsif ($policy eq 'Perl::Critic::Policy::Subroutines::RequireFinalReturn') {
            $output .= "  How to fix: Add explicit 'return;' at the end of subroutines\n";
        } elsif ($policy eq 'Perl::Critic::Policy::RegularExpressions::ProhibitEscapedMetacharacters') {
            $output .= "  How to fix: Use character classes instead of escaping, like [.] instead of \\.\n";
        } elsif ($policy eq 'Perl::Critic::Policy::References::ProhibitDoubleSigils') {
            $output .= "  How to fix: Use proper dereferencing syntax, like \\\$\\\$ref instead of \\\$\\\$\\\$ref\n";
        }
        
        $output .= "\n";
    }
}

my $output_time = tv_interval($output_start);
print STDERR "perlcritic_wrapper: Output generation took ${output_time}s\n";

if ($tidy_check_failed || @filtered_violations) {
    print $output;
    # Save to cache if we have a cache file
    if ($cache_file) {
        my $cache_write_start = [gettimeofday];
        open my $cache_fh, '>', $cache_file or warn "Cannot write to cache file $cache_file: $!";
        print $cache_fh $output;
        close $cache_fh;
        my $cache_write_time = tv_interval($cache_write_start);
        print STDERR "perlcritic_wrapper: Cache write took ${cache_write_time}s\n";
    }
    my $total_time = tv_interval($start_time);
    print STDERR "perlcritic_wrapper: Total time ${total_time}s\n";
    exit 1;
} else {
    $output = "No violations found.\n";
    print $output;
    # Save to cache if we have a cache file
    if ($cache_file) {
        my $cache_write_start = [gettimeofday];
        open my $cache_fh, '>', $cache_file or warn "Cannot write to cache file $cache_file: $!";
        print $cache_fh $output;
        close $cache_fh;
        my $cache_write_time = tv_interval($cache_write_start);
        print STDERR "perlcritic_wrapper: Cache write took ${cache_write_time}s\n";
    }
    my $total_time = tv_interval($start_time);
    print STDERR "perlcritic_wrapper: Total time ${total_time}s\n";
    exit 0;
}

sub check_tidy {
    my $file = shift;
    my $tidy_sub_start = [gettimeofday];
    
    # Read the original file
    open my $fh, '<', $file or die "Cannot open $file: $!";
    my $original = do { local $/; <$fh> };
    close $fh;
    
    # Use the working minimal wrapper approach by calling it as a separate process
    my $perltidy_start = [gettimeofday];
    my $tidy_output = `C:/Strawberry/perl/bin/perl.exe test_wrapper_minimal.pl $file`;
    my $perltidy_time = tv_interval($perltidy_start);
    print STDERR "perlcritic_wrapper: Perltidy subprocess took ${perltidy_time}s\n";
    
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
    
    my $tidy_sub_time = tv_interval($tidy_sub_start);
    print STDERR "perlcritic_wrapper: check_tidy total took ${tidy_sub_time}s\n";
    
    return 0;
}

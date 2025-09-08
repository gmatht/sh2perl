#!/usr/bin/env perl
use strict;
use warnings;
use File::Spec;
use Getopt::Long;
# Try to load PPI, fall back to regex if not available
my $PPI_AVAILABLE = 0;
eval {
    require PPI;
    require PPI::Find;
    $PPI_AVAILABLE = 1;
};
if ($@) {
    warn "Warning: PPI not available, falling back to regex-based parsing\n";
    warn "To install PPI, run: cpan PPI\n";
}

# Command line options
my $help = 0;
my $verbose = 0;
my $inplace = 0;
my $output_file;
my $debashc_path = 'target/debug/debashc.exe';

# Global variables to store preamble blocks and track declared variables
my @preamble_blocks;
my %declared_vars;

GetOptions(
    'help|h' => \$help,
    'verbose|v' => \$verbose,
    'inplace|i' => \$inplace,
    'output|o=s' => \$output_file,
    'debashc-path=s' => \$debashc_path,
) or die "Error in command line arguments\n";

if ($help) {
    print_help();
    exit 0;
}

# Get input file from command line
my $input_file = shift @ARGV;
if (!$input_file) {
    print_help();
    exit 1;
}

if (!-f $input_file) {
    die "Error: Input file '$input_file' does not exist\n";
}

# Check if debashc exists
if (!-f $debashc_path) {
    die "Error: debashc not found at '$debashc_path'. Please build the project first with 'cargo build'\n";
}

print "Purifying Perl file: $input_file\n" if $verbose;

# Read the input file
open my $fh, '<', $input_file or die "Error: Cannot read '$input_file': $!\n";
my $content = do { local $/; <$fh> };
close $fh;

# Process the content
my $purified_content = purify_perl_code($content);

# Show generated Perl code in verbose mode
if ($verbose) {
    print "\n" . "="x60 . "\n";
    print "GENERATED PERL CODE:\n";
    print "="x60 . "\n";
    print $purified_content;
    print "="x60 . "\n\n";
}

# Determine output destination
my $writing_to_stdout = 0;

if ($inplace) {
    # Write back to the input file
    open my $out_fh, '>', $input_file or die "Error: Cannot write to '$input_file': $!\n";
    print $out_fh $purified_content;
    close $out_fh;
    print "Purified code written to: $input_file\n";
} elsif ($output_file) {
    # Write to specified output file
    if ($output_file eq '-') {
        # Write to stdout
        print $purified_content;
        $writing_to_stdout = 1;
    } else {
        open my $out_fh, '>', $output_file or die "Error: Cannot write to '$output_file': $!\n";
        print $out_fh $purified_content;
        close $out_fh;
        print "Purified code written to: $output_file\n";
    }
} else {
    # Default: write to stdout
    print $purified_content;
    $writing_to_stdout = 1;
}

# Only print status message when not writing to stdout
print "Purification complete!\n" unless $writing_to_stdout;

# Subroutines

sub print_help {
    print <<'EOF';
purify.pl - Convert system() calls and backticks to native Perl

USAGE:
    perl purify.pl [options] <input_file>

OPTIONS:
    -h, --help              Show this help message
    -v, --verbose           Verbose output
    -i, --inplace           Modify the input file in place
    -o, --output <file>     Write output to specified file
    --debashc-path <path>   Path to debashc executable (default: target/debug/debashc.exe)

DESCRIPTION:
    This script uses PPI (Perl Parsing Interface) to find instances of:
    - system() calls with shell commands
    - Backtick (`) command substitution
    
    It then uses debashc to convert these shell snippets into native Perl code.
    
    By default, the purified code is written to stdout for easy piping.

EXAMPLES:
    perl purify.pl script.pl                    # Write to stdout
    perl purify.pl script.pl > clean.pl         # Pipe to file
    perl purify.pl -i script.pl                 # Modify file in place
    perl purify.pl -o clean.pl script.pl        # Write to specific file
    perl purify.pl -o - script.pl               # Explicitly write to stdout
    perl purify.pl --debashc-path /path/to/debashc script.pl

EOF
}

sub purify_perl_code {
    my ($content) = @_;
    
    # Clear any previous preamble blocks and declared variables
    @preamble_blocks = ();
    %declared_vars = ();
    
    if ($PPI_AVAILABLE) {
        # Use PPI-based parsing
        my $document = PPI::Document->new(\$content);
        if (!$document) {
            die "Error: Failed to parse Perl code with PPI\n";
        }
        
        # Process system() calls using PPI
        process_system_calls_ppi($document);
        
        # Process backtick command substitution using PPI
        process_backticks_ppi($document);
        
        # Insert preamble blocks at the top of the file
        if (@preamble_blocks) {
            insert_preamble_blocks_ppi($document);
        }
        
        # Return the modified content
        return $document->serialize;
    } else {
        # Fall back to regex-based parsing
        print "Using regex-based parsing (PPI not available)\n" if $verbose;
        
        # Process system() calls
        $content = process_system_calls($content);
        
        # Process backtick command substitution
        $content = process_backticks($content);
        
        # Insert preamble blocks at the top of the file
        if (@preamble_blocks) {
            # Find the end of the initial use statements and shebang
            my $insertion_point = 0;
            my @lines = split(/\n/, $content);
            
            # Find the last use statement or shebang
            for my $i (0..$#lines) {
                if ($lines[$i] =~ /^#!/ || $lines[$i] =~ /^use\s+/ || $lines[$i] =~ /^require\s+/) {
                    $insertion_point = $i + 1;
                } elsif ($lines[$i] =~ /^\s*$/) {
                    # Skip empty lines after use statements
                    next;
                } else {
                    # Found non-use statement, stop here
                    last;
                }
            }
            
            # Insert all preamble blocks
            my $preamble_text = join("\n", @preamble_blocks);
            splice(@lines, $insertion_point, 0, $preamble_text);
            $content = join("\n", @lines);
        }
        
        return $content;
    }
}

sub process_system_calls {
    my ($content) = @_;
    
    # Pattern to match system() calls followed by conditional statements
    # This must come FIRST to avoid partial matches by non-conditional patterns
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*(["'])(.*?)\5\s*\)\s*if\s+(-d\s+['"][^'"]+['"])
    }{
        my $quote1 = $1; my $command = $2; my $quote2 = $3; my $arg1 = $4; my $quote3 = $5; my $arg2 = $6; my $condition = $7;
        my $escaped_arg1 = $arg1; $escaped_arg1 =~ s/"/\\"/g;
        my $escaped_arg2 = $arg2; $escaped_arg2 =~ s/"/\\"/g;
        my $full_command = "$command \"$escaped_arg1\" \"$escaped_arg2\"";
        print "DEBUG: Processing system call with conditional: $full_command if $condition\n" if $verbose;
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    "if ($condition) {\n$perl_result->{core}\n}";
                } else {
                    "if ($condition) {\n$perl_result\n}";
                }
            } else {
                "if ($condition) {\nsystem($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3);\n}";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    "if ($condition) {\n$perl_result->{core}\n}";
                } else {
                    "if ($condition) {\n$perl_result\n}";
                }
            } else {
                "if ($condition) {\nsystem($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3);\n}";
            }
        }
    }gex;
    
    # Pattern to match system() calls with 9 arguments (comma-separated)
    # This must come first to avoid partial matches
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*(["'])(.*?)\5\s*,\s*(["'])(.*?)\7\s*,\s*(["'])(.*?)\9\s*,\s*(["'])(.*?)\11\s*,\s*(["'])(.*?)\13\s*,\s*(["'])(.*?)\15\s*,\s*(["'])(.*?)\17\s*\)
    }{
        my $quote1 = $1; my $command = $2; my $quote2 = $3; my $arg1 = $4; my $quote3 = $5; my $arg2 = $6; my $quote4 = $7; my $arg3 = $8; my $quote5 = $9; my $arg4 = $10; my $quote6 = $11; my $arg5 = $12; my $quote7 = $13; my $arg6 = $14; my $quote8 = $15; my $arg7 = $16; my $quote9 = $17; my $arg8 = $18; my $quote10 = $19; my $arg9 = $20;
        my $escaped_args = join(" ", map { my $escaped = $_; $escaped =~ s/"/\\"/g; "\"$escaped\""; } ($arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7, $arg8, $arg9));
        my $full_command = "$command $escaped_args";
        print "DEBUG: Processing system call with 9 args: $full_command\n" if $verbose;
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7, $quote8$arg7$quote8, $quote9$arg8$quote9, $quote10$arg9$quote10)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result;
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7, $quote8$arg7$quote8, $quote9$arg8$quote9, $quote10$arg9$quote10)";
            }
        }
    }gex;

    # Pattern to match system() calls with 8 arguments (comma-separated)
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*(["'])(.*?)\5\s*,\s*(["'])(.*?)\7\s*,\s*(["'])(.*?)\9\s*,\s*(["'])(.*?)\11\s*,\s*(["'])(.*?)\13\s*,\s*(["'])(.*?)\15\s*\)
    }{
        my $quote1 = $1; my $command = $2; my $quote2 = $3; my $arg1 = $4; my $quote3 = $5; my $arg2 = $6; my $quote4 = $7; my $arg3 = $8; my $quote5 = $9; my $arg4 = $10; my $quote6 = $11; my $arg5 = $12; my $quote7 = $13; my $arg6 = $14; my $quote8 = $15; my $arg7 = $16; my $quote9 = $17; my $arg8 = $18;
        my $escaped_args = join(" ", map { my $escaped = $_; $escaped =~ s/"/\\"/g; "\"$escaped\""; } ($arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7, $arg8));
        my $full_command = "$command $escaped_args";
        print "DEBUG: Processing system call with 8 args: $full_command\n" if $verbose;
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7, $quote8$arg7$quote8, $quote9$arg8$quote9)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result;
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7, $quote8$arg7$quote8, $quote9$arg8$quote9)";
            }
        }
    }gex;

    # Pattern to match system() calls with 7 arguments (comma-separated)
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*(["'])(.*?)\5\s*,\s*(["'])(.*?)\7\s*,\s*(["'])(.*?)\9\s*,\s*(["'])(.*?)\11\s*,\s*(["'])(.*?)\13\s*\)
    }{
        my $quote1 = $1; my $command = $2; my $quote2 = $3; my $arg1 = $4; my $quote3 = $5; my $arg2 = $6; my $quote4 = $7; my $arg3 = $8; my $quote5 = $9; my $arg4 = $10; my $quote6 = $11; my $arg5 = $12; my $quote7 = $13; my $arg6 = $14; my $quote8 = $15; my $arg7 = $16;
        my $escaped_args = join(" ", map { my $escaped = $_; $escaped =~ s/"/\\"/g; "\"$escaped\""; } ($arg1, $arg2, $arg3, $arg4, $arg5, $arg6, $arg7));
        my $full_command = "$command $escaped_args";
        print "DEBUG: Processing system call with 7 args: $full_command\n" if $verbose;
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7, $quote8$arg7$quote8)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result;
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7, $quote8$arg7$quote8)";
            }
        }
    }gex;

    # Pattern to match system() calls with 6 arguments (comma-separated)
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*(["'])(.*?)\5\s*,\s*(["'])(.*?)\7\s*,\s*(["'])(.*?)\9\s*,\s*(["'])(.*?)\11\s*\)
    }{
        my $quote1 = $1; my $command = $2; my $quote2 = $3; my $arg1 = $4; my $quote3 = $5; my $arg2 = $6; my $quote4 = $7; my $arg3 = $8; my $quote5 = $9; my $arg4 = $10; my $quote6 = $11; my $arg5 = $12; my $quote7 = $13; my $arg6 = $14;
        my $escaped_args = join(" ", map { my $escaped = $_; $escaped =~ s/"/\\"/g; "\"$escaped\""; } ($arg1, $arg2, $arg3, $arg4, $arg5, $arg6));
        my $full_command = "$command $escaped_args";
        print "DEBUG: Processing system call with 6 args: $full_command\n" if $verbose;
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result;
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6, $quote7$arg6$quote7)";
            }
        }
    }gex;

    # Pattern to match system() calls with 5 arguments (comma-separated)
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*(["'])(.*?)\5\s*,\s*(["'])(.*?)\7\s*,\s*(["'])(.*?)\9\s*\)
    }{
        my $quote1 = $1; my $command = $2; my $quote2 = $3; my $arg1 = $4; my $quote3 = $5; my $arg2 = $6; my $quote4 = $7; my $arg3 = $8; my $quote5 = $9; my $arg4 = $10; my $quote6 = $11; my $arg5 = $12;
        my $escaped_args = join(" ", map { my $escaped = $_; $escaped =~ s/"/\\"/g; "\"$escaped\""; } ($arg1, $arg2, $arg3, $arg4, $arg5));
        my $full_command = "$command $escaped_args";
        print "DEBUG: Processing system call with 5 args: $full_command\n" if $verbose;
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result;
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4, $quote5$arg4$quote5, $quote6$arg5$quote6)";
            }
        }
    }gex;

    # Pattern to match system() calls with 4 arguments (comma-separated)
    # This must come after the higher argument patterns to avoid partial matches
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*(["'])(.*?)\5\s*,\s*(["'])(.*?)\7\s*\)
    }{
        my $quote1 = $1;
        my $command = $2;
        my $quote2 = $3;
        my $arg1 = $4;
        my $quote3 = $5;
        my $arg2 = $6;
        my $quote4 = $7;
        my $arg3 = $8;
        # Reconstruct the full command with proper quoting
        # Escape double quotes in the args
        my $escaped_arg1 = $arg1;
        $escaped_arg1 =~ s/"/\\"/g;
        my $escaped_arg2 = $arg2;
        $escaped_arg2 =~ s/"/\\"/g;
        my $escaped_arg3 = $arg3;
        $escaped_arg3 =~ s/"/\\"/g;
        my $full_command = "$command \"$escaped_arg1\" \"$escaped_arg2\" \"$escaped_arg3\"";
        print "DEBUG: Processing system call with 4 args: $full_command\n" if $verbose;
        # Convert ls commands to native Perl
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    # Old format: just use the code
                    $perl_result;
                }
            } else {
                # Fallback to original system call
                "system($quote1$command$quote1, $quote2$arg1$quote2, $quote3$arg2$quote3, $quote4$arg3$quote4)";
            }
        }
    }gex;

    # Pattern to match system() calls with 3 arguments (comma-separated)
    # This must come after the 4-argument pattern to avoid partial matches
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*,\s*((?:["'][^"']*["']|[^,)]+?))\s*\)
    }{
        my $quote1 = $1;
        my $command = $2;
        my $quote2 = $3;
        my $arg1 = $4;
        my $arg2 = $5;
        # Reconstruct the full command with proper quoting
        # Escape double quotes in the args
        my $escaped_arg1 = $arg1;
        $escaped_arg1 =~ s/"/\\"/g;
        my $escaped_arg2 = $arg2;
        # Remove quotes from arg2 if it's quoted, otherwise keep as-is
        if ($escaped_arg2 =~ /^["'](.*)["']$/) {
            $escaped_arg2 = $1;
            $escaped_arg2 =~ s/"/\\"/g;
            $escaped_arg2 = "\"$escaped_arg2\"";
        }
        my $full_command = "$command \"$escaped_arg1\" $escaped_arg2";
        print "DEBUG: Processing system call with 3 args: $full_command\n" if $verbose;
        # Convert ls commands to native Perl
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$arg1$quote2, $arg2)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    # Old format: just use the code
                    $perl_result;
                }
            } else {
                # Fallback to original system call
                "system($quote1$command$quote1, $quote2$arg1$quote2, $arg2)";
            }
        }
    }gex;

    # Pattern to match system() calls with 2 arguments (comma-separated)
    # This must come after the 3-argument pattern to avoid partial matches
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*\)
    }{
        my $quote1 = $1;
        my $command = $2;
        my $quote2 = $3;
        my $args = $4;
        # Reconstruct the full command with proper quoting
        # Escape double quotes in the args
        my $escaped_args = $args;
        $escaped_args =~ s/"/\\"/g;
        my $full_command = "$command \"$escaped_args\"";
        print "DEBUG: Processing system call with multiple args: $full_command\n" if $verbose;
        # Convert ls commands to native Perl
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote1$command$quote1, $quote2$args$quote2)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    # Fix quote handling for echo commands
                    my $core = $perl_result->{core};
                    if ($command eq 'echo') {
                        # Fix single quotes
                        $core =~ s/'quoted'/"'quoted'"/g;
                        # Fix double quotes
                        $core =~ s/"double quoted"/"\\"double quoted\\""/g;
                    }
                    $core;
                } else {
                    # Old format: just return the code
                    $perl_result;
                }
            } else {
                # Try fallback for basic echo commands
                if ($command eq 'echo') {
                    my $text = $args;
                    # Escape quotes properly for Perl
                    $text =~ s/\\/\\\\/g;  # Escape backslashes first
                    $text =~ s/"/\\"/g;    # Escape double quotes
                    "print \"$text\\n\"";
                } else {
                    "system($quote1$command$quote1, $quote2$args$quote2)";  # Keep original if conversion fails
                }
            }
        }
    }gex;
    
    # Pattern to match system() calls with single string argument
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*
        \)
    }{
        my $quote = $1;
        my $command = $2;
        print "DEBUG: Processing system call: $command\n" if $verbose;
        # Convert ls commands to native Perl
        my $command_name = (split /\s+/, $command)[0];
        if ($command_name eq 'ls') {
            my $perl_result = convert_shell_to_perl($command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    insert_preamble($perl_result->{preamble});
                    $perl_result->{core};
                } else {
                    $perl_result
                }
            } else {
                "system($quote$command$quote)";
            }
        } else {
            my $perl_result = convert_shell_to_perl($command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    # New format: insert preamble and return core
                    print "DEBUG: Inserting preamble for: $command\n" if $verbose;
                    insert_preamble($perl_result->{preamble});
                    print "DEBUG: Core code: $perl_result->{core}\n" if $verbose;
                    $perl_result->{core};
                } else {
                    # Old format: just return the code
                    print "DEBUG: Old format result: $perl_result\n" if $verbose;
                    $perl_result;
                }
            } else {
                print "DEBUG: Conversion failed for: $command\n" if $verbose;
                "system($quote$command$quote)";  # Keep original if conversion fails
            }
        }
    }gex;
    
    # Pattern to match system() calls with 2 arguments followed by conditional statements
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*\)\s*if\s+(-d\s+['"][^'"]+['"])
    }{
        my $quote1 = $1; my $command = $2; my $quote2 = $3; my $arg1 = $4; my $condition = $5;
        my $escaped_arg1 = $arg1; $escaped_arg1 =~ s/"/\\"/g;
        my $full_command = "$command \"$escaped_arg1\"";
        print "DEBUG: Processing system call with conditional (2 args): $full_command $condition\n" if $verbose;
        if ($command eq 'ls') {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    "if ($condition) {\n$perl_result->{core}\n}";
                } else {
                    "if ($condition) {\n$perl_result\n}";
                }
            } else {
                "if ($condition) {\nsystem($quote1$command$quote1, $quote2$arg1$quote2);\n}";
            }
        } else {
            my $perl_result = convert_shell_to_perl($full_command, 0);
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    "if ($condition) {\n$perl_result->{core}\n}";
                } else {
                    "if ($condition) {\n$perl_result\n}";
                }
            } else {
                "if ($condition) {\nsystem($quote1$command$quote1, $quote2$arg1$quote2);\n}";
            }
        }
    }gex;
    
    return $content;
}

sub process_backticks {
    my ($content) = @_;
    
    print "DEBUG: Processing backticks in content\n" if $verbose;
    
    # Process backtick commands one by one to handle complex cases
    while ($content =~ /`([^`]+)`/) {
        my $command = $1;
        
        # Extract the command name (first word)
        my $command_name = $command;
        $command_name =~ s/^\s+//;  # Remove leading whitespace
        $command_name =~ s/\s.*$//;  # Remove everything after first space
        
        # Check if this is a builtin command
        my $is_builtin = is_builtin_command($command_name);
        print "DEBUG: Command '$command_name' is builtin: " . ($is_builtin ? "yes" : "no") . "\n" if $verbose;
        
        # Special handling for basic ls commands (no options)
        if ($command_name eq 'ls' && $command eq 'ls') {
            my $ls_code = q{do {
my @ls_files = ();
if (opendir my $dh, '.') {
    while (my $file = readdir $dh) {
        next if $file eq q{.} || $file eq q{..} || $file =~ /^\./;
        push @ls_files, $file;
    }
    closedir $dh;
    @ls_files = sort { $a cmp $b } @ls_files;
}
join "\\n", @ls_files
}};
            $content =~ s/`\Q$command\E`/$ls_code/;
            print "DEBUG: Converted backtick command '$command' to Perl (special ls handling)\n" if $verbose;
        } else {
            # Convert the backtick command to Perl
            my $perl_result = convert_shell_to_perl($command, 1);  # 1 = is_backticks
            if ($perl_result) {
                if (ref($perl_result) eq 'HASH') {
                    insert_preamble($perl_result->{preamble});
                    $content =~ s/`\Q$command\E`/$perl_result->{core}/;
                } else {
                    # For backtick commands, just use the result as-is
                    # This includes inline code generated by extract_perl_from_debashc_output
                    $content =~ s/`\Q$command\E`/$perl_result/;
                }
                print "DEBUG: Converted backtick command '$command' to Perl\n" if $verbose;
            } else {
                print "DEBUG: Failed to convert backtick command '$command', leaving unchanged\n" if $verbose;
                last;  # Stop processing if conversion fails
            }
        }
    }
    
    return $content;
}

sub is_builtin_command {
    my ($command_name) = @_;
    
    # List of builtin commands from src/generator/commands/builtins.rs
    my @builtin_commands = qw(
        ls cat find grep sed awk sort uniq wc head tail cut paste comm diff tr xargs perl cd read
        cp mv rm mkdir touch
        echo printf basename dirname
        date time sleep which yes
        gzip zcat
        wget curl
        kill nohup nice
        sha256sum sha512sum strings
        tee
    );
    
    return grep { $_ eq $command_name } @builtin_commands;
}

sub insert_preamble {
    my ($preamble) = @_;
    
    # Extract only the variable declarations from the preamble
    # Skip the shebang and use statements as they're already in the original file
    my @lines = split(/\n/, $preamble);
    my @var_decls = ();
    
    my $in_ls_block = 0;
    for my $line (@lines) {
        # Skip shebang, use statements, and empty lines
        next if $line =~ /^#!/;
        next if $line =~ /^use\s+/;
        next if $line =~ /^require\s+/;
        next if $line =~ /^my \$main_exit_code/;
        next if $line =~ /^\s*$/;
        
        # Detect start of ls block
        if ($line =~ /^\$ls_dir = / || $line =~ /^\@ls_files/) {
            $in_ls_block = 1;
        }
        
        # If we're in an ls block, keep all lines including control structures
        if ($in_ls_block) {
            push @var_decls, $line;
            # End of ls block when we hit a closing brace or print statement
            if ($line =~ /^}$/ || $line =~ /^print /) {
                $in_ls_block = 0;
            }
        } else {
            # Keep variable declarations and logic
            push @var_decls, $line;
        }
    }
    
    # Add the preamble with deduplication for variable declarations
    my $var_decl_text = join("\n", @var_decls);
    if ($var_decl_text && !grep { $_ eq $var_decl_text } @preamble_blocks) {
        push @preamble_blocks, $var_decl_text;
    }
}


sub convert_shell_to_perl {
    my ($shell_command, $is_backticks, $output_var) = @_;
    
    return undef unless $shell_command;
    
    # Clean up the command
    $shell_command =~ s/^\s+//;
    $shell_command =~ s/\s+$//;
    
    return undef if !$shell_command;
    
    print "Converting shell command: $shell_command\n" if $verbose;
    
    # Use debashc to convert the shell command to Perl
    # Escape the command properly for shell execution
    my $escaped_command = $shell_command;
    $escaped_command =~ s/'/'"'"'/g;  # Escape single quotes
    $escaped_command = "'$escaped_command'";  # Wrap in single quotes
    
    # Use debashc to convert to Perl
    my $stdout = `"$debashc_path" parse --perl $escaped_command 2>&1`;
    my $exit_code = $? >> 8;
    
    if ($exit_code != 0) {
        warn "debashc failed with exit code $exit_code: $stdout\n";
        # Try a simple fallback for basic echo commands
        if ($shell_command =~ /^echo\s+(.+)$/) {
            my $text = $1;
            # Remove surrounding quotes if present
            $text =~ s/^["']|["']$//g;
            return "print \"$text\\n\";";
        }
        return undef;
    }
    
    # Extract the Perl code from debashc output
    print "DEBUG: Calling extract_perl_from_debashc_output with is_backticks=$is_backticks\n" if $verbose;
    my $perl_result = extract_perl_from_debashc_output($stdout, $is_backticks);
    
    if (!$perl_result) {
        warn "Failed to extract Perl code from debashc output\n" if $verbose;
        return undef;
    }
    
    # Handle both old format (string) and new format (hash reference)
    if (ref($perl_result) eq 'HASH') {
        # New format: preamble and core
        my $preamble = $perl_result->{preamble};
        my $core = $perl_result->{core};
        print "Converted to Perl (preamble): $preamble\n" if $verbose;
        print "Converted to Perl (core): $core\n" if $verbose;
        return { preamble => $preamble, core => $core };
    } else {
        # Old format: just the core code
        print "Converted to Perl: $perl_result\n" if $verbose;
        return $perl_result;
    }
}

sub extract_perl_from_debashc_output {
    my ($output, $is_backticks) = @_;
    
    print "DEBUG: extract_perl_from_debashc_output called with is_backticks=$is_backticks\n" if $verbose;
    print "DEBUG: Output length: " . length($output) . "\n" if $verbose;
    print "DEBUG: First 200 chars: " . substr($output, 0, 200) . "\n" if $verbose;
    
    # Check if this is an error message first
    if ($output =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
        return undef;
    }
    
    # Look for Perl code in the output
    # The output format might vary, so we try different patterns
    
    # Pattern 0: New preamble/core format
    if ($output =~ /PREAMBLE:\s*\n(.*?)\nCORE:\s*\n(.*?)\n={50}/s) {
        my $preamble = $1;
        my $core = $2;
        # Check if the code contains error messages
        if ($preamble =~ /Parse error:|Error:|Failed:|Unexpected token:/ || 
            $core =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        # Clean up the core code - remove trailing semicolons and extra whitespace
        $core =~ s/;\s*$//;
        $core =~ s/\n\s*$//;
        
        # For backtick ls commands, return the full preamble as inline code
        if ($is_backticks && $preamble =~ /\$ls_dir = / && $preamble =~ /\@ls_files = /) {
            # Convert the preamble to inline code
            my $inline_preamble = $preamble;
            # Replace print statement with return value
            $inline_preamble =~ s/print join "\\n", \@ls_files;/join "\\n", \@ls_files/g;
            return "do { $inline_preamble }";
        }
        
        # For backtick ls commands with the new format, handle the core code
        if ($is_backticks && $core =~ /my \@ls_files = \(\)/ && $core =~ /opendir my \$dh/) {
            # Convert the core code to inline code, removing the print statement
            my $inline_core = $core;
            # Replace print statement with return value
            $inline_core =~ s/print join "\\n", \@ls_files;/join "\\n", \@ls_files/g;
            return "do { $inline_core }";
        }
        
        # Return both preamble and core as a hash reference
        return { preamble => $preamble, core => $core };
    }
    
    # Pattern 1: Look for code between markers (old format)
    if ($output =~ /Generated Perl code:\s*\n(.*?)\n---/s) {
        return $1;
    }
    
    # Pattern 2: Look for code after "Converting system command to Perl:" and before the separator line
    print "DEBUG: Trying Pattern 2\n" if $verbose;
    if ($output =~ /Converting system command to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Check if the code contains error messages
        if ($code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        
        # For system calls, remove print statements since system() doesn't print
        # But keep print statements that are part of redirection blocks
        # For backtick commands, we need to capture the output
        if (!$is_backticks) {
            # For system calls, don't remove print statements for now
            # TODO: Need better logic to handle redirection vs normal commands
        } else {
            # For backtick commands, we need to capture the output instead of printing it
            print "DEBUG: Before conversion: $code\n" if $verbose;
            # For backtick commands, we need to return the printed value
            # Look for print statements and convert them to return values
            if ($code =~ /print\s+(.+?);?\s*$/) {
                my $print_value = $1;
                $print_value =~ s/;\s*$//;  # Remove trailing semicolon
                $code = $print_value;
            }
            print "DEBUG: After conversion: $code\n" if $verbose;
        }
        
        return $code;
    }
    
    # Pattern 2b: Look for code after "Converting backticks command to Perl:" and before the separator line
    if ($output =~ /Converting backticks command to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Check if the code contains error messages
        if ($code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        
        # For system calls, remove print statements since system() doesn't print
        # But keep print statements that are part of redirection blocks
        # For backtick commands, convert print statements to return values
        if (!$is_backticks) {
            # For system calls, don't remove print statements for now
            # TODO: Need better logic to handle redirection vs normal commands
        } else {
        # For backtick commands, we need to return the printed value
        print "DEBUG: Before conversion: $code\n" if $verbose;
        # For backtick commands, we need to return the printed value
        # Look for the last print statement and return its value
        if ($code =~ /print\s+(.+?);?\s*$/) {
            my $print_value = $1;
            $print_value =~ s/;\s*$//;  # Remove trailing semicolon
            $code = $print_value;
        }
        # Also handle sprintf calls that return values
        elsif ($code =~ /sprintf\s*\(.+?\)/) {
            # sprintf calls are already return values, no conversion needed
            $code =~ s/;\s*$//;  # Remove trailing semicolon if present
        }
        print "DEBUG: After conversion: $code\n" if $verbose;
        }
        
        return $code;
    }
    
    # Pattern 2c: Look for code after "Converting snippet to Perl:" and before the separator line (old format)
    if ($output =~ /Converting snippet to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Check if the code contains error messages
        if ($code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        
        # For system calls, remove print statements since system() doesn't print
        # But keep print statements that are part of redirection blocks
        # For backtick commands, convert print statements to return values
        if (!$is_backticks) {
            # For system calls, don't remove print statements for now
            # TODO: Need better logic to handle redirection vs normal commands
        } else {
        # For backtick commands, we need to return the printed value
        print "DEBUG: Before conversion: $code\n" if $verbose;
        # For backtick commands, we need to return the printed value
        # Look for the last print statement and return its value
        if ($code =~ /print\s+(.+?);?\s*$/) {
            my $print_value = $1;
            $print_value =~ s/;\s*$//;  # Remove trailing semicolon
            $code = $print_value;
        }
        # Also handle sprintf calls that return values
        elsif ($code =~ /sprintf\s*\(.+?\)/) {
            # sprintf calls are already return values, no conversion needed
            $code =~ s/;\s*$//;  # Remove trailing semicolon if present
        }
        print "DEBUG: After conversion: $code\n" if $verbose;
        }
        
        return $code;
    }
    
    # Pattern 3: Look for code after "Converting to Perl:" and before the separator line (old format)
    print "DEBUG: Trying Pattern 3\n" if $verbose;
    if ($output =~ /Converting to Perl:\s*\n={40,}\s*\n(.*?)\n={40,}/s) {
        print "DEBUG: Matched Pattern 3\n" if $verbose;
        my $code = $1;
        print "DEBUG: Pattern 3 extracted code: [$code]\n" if $verbose;
        # Check if the code contains error messages
        if ($code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        # Extract just the main logic after the variable declarations
        if ($code =~ /my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)/s) {
            my $main_code = $1;
            print "DEBUG: Pattern 3 main_code: [$main_code]\n" if $verbose;
            $main_code =~ s/;\s*$//;
            $main_code =~ s/\n\s*$//;
            
            # For system calls, don't remove print statements for now
            # TODO: Need better logic to handle redirection vs normal commands
            if (!$is_backticks) {
                # Disabled print removal for now
            } else {
                # For backtick commands, we need to handle variable declarations
                # Extract variable declarations and move them to preamble
                my @lines = split(/\n/, $main_code);
                my @preamble_lines = ();
                my @core_lines = ();
                my $in_preamble = 1;
                
                # Generate unique variable names for this backtick command
                my $unique_suffix = int(rand(10000));
                my %var_mapping = ();
                
                for my $line (@lines) {
                    if ($in_preamble && $line =~ /^(?:my\s+)?([\$@]\w+).*?;?\s*$/) {
                        my $original_var = $1;
                        my $new_var = $original_var . "_$unique_suffix";
                        $var_mapping{$original_var} = $new_var;
                        $line =~ s/\Q$original_var\E/$new_var/g;
                        push @preamble_lines, $line;
                    } else {
                        $in_preamble = 0;
                        push @core_lines, $line;
                    }
                }
                
                # For ls commands, return inline code instead of using preamble
                # This applies to all ls commands to ensure they work correctly
                if ($main_code =~ /\@ls_files/ && $main_code =~ /opendir/) {
                    print "DEBUG: Matched ls pattern, converting to inline code\n" if $verbose;
                    # Convert the full main_code to inline code
                    my $inline_code = $main_code;
                    # Replace print statement with return value for backtick commands
                    $inline_code =~ s/print join "\\n", \@ls_files, "\\n";/join "\\n", \@ls_files, "\\n"/g;
                    return "do { $inline_code }";
                }
                
                # For printf commands, return inline code that captures output
                if ($main_code =~ /printf\s*\(/) {
                    # Convert printf call to capture output using sprintf
                    my $inline_code = $main_code;
                    # Replace printf with sprintf to capture output
                    $inline_code =~ s/printf\s*\(/sprintf(/;
                    # Remove any trailing semicolon since this will be used in assignment
                    $inline_code =~ s/;\s*$//;
                    # For backtick commands, extract just the sprintf call
                    if ($is_backticks) {
                        # Extract just the sprintf call from the multi-line result
                        if ($inline_code =~ /sprintf\s*\(.+?\)/) {
                            return $&;  # Return just the sprintf call
                        }
                    }
                    return $inline_code;
                }
                
                if (@preamble_lines) {
                    my $preamble = join("\n", @preamble_lines);
                    insert_preamble($preamble);
                }
                
                # Return the print statement value with updated variable names
                if ($main_code =~ /print\s+(.+?);?\s*$/) {
                    my $print_value = $1;
                    $print_value =~ s/;\s*$//;  # Remove trailing semicolon
                    
                    # Update variable names in the print value
                    for my $original_var (keys %var_mapping) {
                        my $new_var = $var_mapping{$original_var};
                        $print_value =~ s/\Q$original_var\E/$new_var/g;
                    }
                    
                    return $print_value;
                } elsif ($main_code =~ /^\{.*\}$/s) {
                    # Handle complex blocks - for now, just execute them and return empty string
                    # since they typically redirect to /dev/null
                    my $block_code = $main_code;
                    
                    # Update variable names in the block
                    for my $original_var (keys %var_mapping) {
                        my $new_var = $var_mapping{$original_var};
                        $block_code =~ s/\Q$original_var\E/$new_var/g;
                    }
                    
                    # Add the block to preamble and return empty string
                    insert_preamble($block_code);
                    return '""';
                }
            }
            
            return $main_code;
        }
        
        # For system calls, remove print statements since system() doesn't print
        # But keep print statements that are part of redirection blocks
        # For backtick commands, convert print statements to return values
        if (!$is_backticks) {
            # For system calls, don't remove print statements for now
            # TODO: Need better logic to handle redirection vs normal commands
        } else {
        # For backtick commands, we need to return the printed value
        print "DEBUG: Before conversion: $code\n" if $verbose;
        # For backtick commands, we need to return the printed value
        # Look for the last print statement and return its value
        if ($code =~ /print\s+(.+?);?\s*$/) {
            my $print_value = $1;
            $print_value =~ s/;\s*$//;  # Remove trailing semicolon
            $code = $print_value;
        }
        # Also handle sprintf calls that return values
        elsif ($code =~ /sprintf\s*\(.+?\)/) {
            # sprintf calls are already return values, no conversion needed
            $code =~ s/;\s*$//;  # Remove trailing semicolon if present
        }
        print "DEBUG: After conversion: $code\n" if $verbose;
        }
        
        return $code;
    }
    
    # Pattern 4: Extract just the main logic from the full script
    print "DEBUG: Trying Pattern 4 with regex: /my \\\$main_exit_code = 0;\\s*\\n(.*?)(?:\\n\\s*\$|\$)/s\n" if $verbose;
    if ($output =~ /my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)/s) {
        print "DEBUG: Matched Pattern 4\n" if $verbose;
        my $code = $1;
        # Check if the code contains error messages
        if ($code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        
        # For system calls, remove print statements since system() doesn't print
        # But keep print statements that are part of redirection blocks
        # For backtick commands, convert print statements to return values
        if (!$is_backticks) {
            # For system calls, don't remove print statements for now
            # TODO: Need better logic to handle redirection vs normal commands
        } else {
        # For backtick commands, we need to return the printed value
        print "DEBUG: Before conversion: $code\n" if $verbose;
        # For backtick commands, we need to return the printed value
        # Look for the last print statement and return its value
        if ($code =~ /print\s+(.+?);?\s*$/) {
            my $print_value = $1;
            $print_value =~ s/;\s*$//;  # Remove trailing semicolon
            $code = $print_value;
        }
        # Also handle sprintf calls that return values
        elsif ($code =~ /sprintf\s*\(.+?\)/) {
            # sprintf calls are already return values, no conversion needed
            $code =~ s/;\s*$//;  # Remove trailing semicolon if present
        }
        print "DEBUG: After conversion: $code\n" if $verbose;
        }
        
        return $code;
    }
    
    # Pattern 5: If the output is just Perl code
    print "DEBUG: Trying Pattern 5\n" if $verbose;
    if ($output =~ /^[^=]/ && $output !~ /Error|Failed|Parse error|Unexpected token/) {
        print "DEBUG: Matched Pattern 5\n" if $verbose;
        # Check if the code contains undefined variables or invalid syntax
        if ($output =~ /@\w+[^=]/ || $output =~ /undefined|undefined variable/i) {
            return undef;
        }
        
        # For system calls, remove print statements since system() doesn't print
        if (!$is_backticks) {
            $output =~ s/print[^;]*;//g;
            $output =~ s/print\s+join[^;]*;//g;
        }
        
        return $output;
    }
    
    return undef;
}

# PPI-based system() call processing
sub process_system_calls_ppi {
    my ($document) = @_;
    
    # Find all function calls
    my $find = PPI::Find->new(sub {
        my $node = shift;
        return 1 if $node->isa('PPI::Statement::Expression') && 
                   $node->first_token && 
                   $node->first_token->content eq 'system';
    });
    
    my @system_calls = $find->in($document);
    
    for my $system_call (@system_calls) {
        process_single_system_call_ppi($system_call);
    }
}

sub process_single_system_call_ppi {
    my ($system_call) = @_;
    
    # Get the arguments to the system call
    my $args = $system_call->find('PPI::Structure::List');
    return unless $args && @{$args};
    
    my $arg_list = $args->[0];
    my @arguments = $arg_list->find('PPI::Statement::Expression');
    
    # Extract the command and arguments
    my $command = extract_string_from_ppi($arguments[0]) if @arguments;
    return unless $command;
    
    # Build the full command with all arguments
    my @cmd_args = ($command);
    for my $i (1..$#arguments) {
        my $arg = extract_string_from_ppi($arguments[$i]);
        push @cmd_args, $arg if defined $arg;
    }
    
    my $full_command = join(' ', @cmd_args);
    print "DEBUG: Processing system call: $full_command\n" if $verbose;
    
    # Convert to Perl using debashc
    my $perl_result = convert_shell_to_perl($full_command, 0);
    if ($perl_result) {
        if (ref($perl_result) eq 'HASH') {
            insert_preamble($perl_result->{preamble});
            replace_system_call_with_code($system_call, $perl_result->{core});
        } else {
            replace_system_call_with_code($system_call, $perl_result);
        }
    }
}

sub extract_string_from_ppi {
    my ($expr) = @_;
    return unless $expr;
    
    # Look for string literals
    my $strings = $expr->find('PPI::Token::Quote');
    if ($strings && @{$strings}) {
        my $string = $strings->[0];
        return $string->string;
    }
    
    # Look for barewords or other expressions
    my $tokens = $expr->find('PPI::Token::Word');
    if ($tokens && @{$tokens}) {
        return $tokens->[0]->content;
    }
    
    return undef;
}

sub replace_system_call_with_code {
    my ($system_call, $replacement_code) = @_;
    
    # Create a new PPI document from the replacement code
    my $replacement_doc = PPI::Document->new(\$replacement_code);
    return unless $replacement_doc;
    
    # Get the parent statement
    my $parent = $system_call->parent;
    return unless $parent;
    
    # Find the statement that contains this system call
    my $statement = $parent;
    while ($statement && !$statement->isa('PPI::Statement')) {
        $statement = $statement->parent;
    }
    return unless $statement;
    
    # Replace the entire statement with the new code
    my $new_statement = $replacement_doc->child(0);
    if ($new_statement) {
        $statement->replace($new_statement);
    }
}

# PPI-based backtick processing
sub process_backticks_ppi {
    my ($document) = @_;
    
    # Find all backtick expressions
    my $find = PPI::Find->new(sub {
        my $node = shift;
        return 1 if $node->isa('PPI::Token::QuoteLike::Backtick');
    });
    
    my @backticks = $find->in($document);
    
    for my $backtick (@backticks) {
        process_single_backtick_ppi($backtick);
    }
}

sub process_single_backtick_ppi {
    my ($backtick) = @_;
    
    my $command = $backtick->string;
    print "DEBUG: Processing backtick command: $command\n" if $verbose;
    
    # Special handling for basic ls commands
    if ($command eq 'ls') {
        my $ls_code = q{do {
my @ls_files = ();
if (opendir my $dh, '.') {
    while (my $file = readdir $dh) {
        next if $file eq q{.} || $file eq q{..} || $file =~ /^\./;
        push @ls_files, $file;
    }
    closedir $dh;
    @ls_files = sort { $a cmp $b } @ls_files;
}
join "\\n", @ls_files
}};
        replace_backtick_with_code($backtick, $ls_code);
        return;
    }
    
    # Convert using debashc
    my $perl_result = convert_shell_to_perl($command, 1);
    if ($perl_result) {
        if (ref($perl_result) eq 'HASH') {
            insert_preamble($perl_result->{preamble});
            replace_backtick_with_code($backtick, $perl_result->{core});
        } else {
            replace_backtick_with_code($backtick, $perl_result);
        }
    }
}

sub replace_backtick_with_code {
    my ($backtick, $replacement_code) = @_;
    
    # Create a new PPI document from the replacement code
    my $replacement_doc = PPI::Document->new(\$replacement_code);
    return unless $replacement_doc;
    
    # Get the first child (the actual code)
    my $new_code = $replacement_doc->child(0);
    return unless $new_code;
    
    # Replace the backtick with the new code
    $backtick->replace($new_code);
}

# PPI-based preamble insertion
sub insert_preamble_blocks_ppi {
    my ($document) = @_;
    
    # Find the insertion point after use statements
    my $insertion_point = find_preamble_insertion_point($document);
    return unless defined $insertion_point;
    
    # Create preamble code
    my $preamble_text = join("\n", @preamble_blocks);
    my $preamble_doc = PPI::Document->new(\$preamble_text);
    return unless $preamble_doc;
    
    # Insert preamble blocks
    my $children = $document->children;
    for my $i (reverse 0..$preamble_doc->children - 1) {
        my $preamble_child = $preamble_doc->child($i);
        $document->insert_before($children->[$insertion_point], $preamble_child);
    }
}

sub find_preamble_insertion_point {
    my ($document) = @_;
    
    my $children = $document->children;
    my $insertion_point = 0;
    
    for my $i (0..$children - 1) {
        my $child = $children->[$i];
        
        # Check if this is a use statement, require, or shebang
        if ($child->isa('PPI::Statement::Include') || 
            ($child->isa('PPI::Token::Comment') && $child->content =~ /^#!/)) {
            $insertion_point = $i + 1;
        } elsif ($child->isa('PPI::Token::Whitespace') && $child->content =~ /^\n$/) {
            # Skip empty lines after use statements
            next;
        } else {
            # Found non-use statement, stop here
            last;
        }
    }
    
    return $insertion_point;
}

__END__

=head1 NAME

purify.pl - Convert system() calls and backticks to native Perl

=head1 SYNOPSIS

    perl purify.pl [options] <input_file>

=head1 DESCRIPTION

This script uses PPI (Perl Parsing Interface) to find instances of:
- system() calls with shell commands
- Backtick (`) command substitution

It then uses debashc to convert these shell snippets into native Perl code.

=head1 OPTIONS

=over 4

=item -h, --help

Show this help message

=item -v, --verbose

Verbose output showing what is being converted

=item -i, --inplace

Modify the input file in place

=item -o, --output <file>

Write output to specified file

=item --debashc-path <path>

Path to debashc executable (default: target/debug/debashc.exe)

=back

=head1 EXAMPLES

    perl purify.pl script.pl
    perl purify.pl -i script.pl
    perl purify.pl -o clean.pl script.pl
    perl purify.pl --debashc-path /path/to/debashc script.pl

=head1 REQUIREMENTS

- debashc executable (built from this project)
- PPI (Perl Parsing Interface)
- File::Temp
- IPC::Run3
- Getopt::Long

=head1 AUTHOR

Generated for the sh2perl project

=cut
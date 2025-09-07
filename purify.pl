#!/usr/bin/env perl
use strict;
use warnings;
use File::Spec;
use Getopt::Long;

# Command line options
my $help = 0;
my $verbose = 0;
my $inplace = 0;
my $output_file;
my $debashc_path = 'target/debug/debashc.exe';

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
    This script uses regex-based parsing to find instances of:
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
    
    # Process system() calls
    $content = process_system_calls($content);
    
    # Process backtick command substitution
    $content = process_backticks($content);
    
    return $content;
}

sub process_system_calls {
    my ($content) = @_;
    
    # Pattern to match system() calls with multiple arguments (comma-separated)
    # This must come first to avoid partial matches
    $content =~ s{
        system\s*\(\s*
        (["'])(.*?)\1\s*,\s*(["'])(.*?)\3\s*\)
    }{
        my $quote1 = $1;
        my $command = $2;
        my $quote2 = $3;
        my $args = $4;
        # Reconstruct the full command
        my $full_command = "$command $args";
        my $perl_code = convert_shell_to_perl($full_command, 0);
        if ($perl_code) {
            $perl_code;
        } else {
            "system($quote1$command$quote1, $quote2$args$quote2)";  # Keep original if conversion fails
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
        my $perl_code = convert_shell_to_perl($command, 0);
        if ($perl_code) {
            $perl_code;
        } else {
            "system($quote$command$quote)";  # Keep original if conversion fails
        }
    }gex;
    
    return $content;
}

sub process_backticks {
    my ($content) = @_;
    
    # Pattern to match backtick command substitution
    $content =~ s{
        `([^`]+)`
    }{
        my $command = $1;
        my $perl_code = convert_shell_to_perl($command, 1);
        if ($perl_code) {
            # Wrap in parentheses to maintain precedence
            "($perl_code)";
        } else {
            "`$command`";  # Keep original if conversion fails
        }
    }gex;
    
    return $content;
}

sub convert_shell_to_perl {
    my ($shell_command, $is_backticks) = @_;
    
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
    
    # Choose the appropriate debashc option
    my $option = $is_backticks ? "--backticks" : "--system";
    my $stdout = `"$debashc_path" parse $option $escaped_command 2>&1`;
    my $exit_code = $? >> 8;
    
    if ($exit_code != 0) {
        warn "debashc failed with exit code $exit_code: $stdout\n";
        return undef;
    }
    
    # Extract the Perl code from debashc output
    my $perl_code = extract_perl_from_debashc_output($stdout);
    
    if (!$perl_code) {
        warn "Failed to extract Perl code from debashc output\n";
        return undef;
    }
    
    print "Converted to Perl: $perl_code\n" if $verbose;
    return $perl_code;
}

sub extract_perl_from_debashc_output {
    my ($output) = @_;
    
    # Look for Perl code in the output
    # The output format might vary, so we try different patterns
    
    # Pattern 1: Look for code between markers (old format)
    if ($output =~ /Generated Perl code:\s*\n(.*?)\n---/s) {
        return $1;
    }
    
    # Pattern 2: Look for code after "Converting system command to Perl:" and before the separator line
    if ($output =~ /Converting system command to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        return $code;
    }
    
    # Pattern 2b: Look for code after "Converting backticks command to Perl:" and before the separator line
    if ($output =~ /Converting backticks command to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        return $code;
    }
    
    # Pattern 2c: Look for code after "Converting snippet to Perl:" and before the separator line (old format)
    if ($output =~ /Converting snippet to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        return $code;
    }
    
    # Pattern 3: Look for code after "Converting to Perl:" and before the separator line (old format)
    if ($output =~ /Converting to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Extract just the main logic after the variable declarations
        if ($code =~ /my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)/s) {
            my $main_code = $1;
            $main_code =~ s/;\s*$//;
            $main_code =~ s/\n\s*$//;
            return $main_code;
        }
        return $code;
    }
    
    # Pattern 4: Extract just the main logic from the full script
    if ($output =~ /my \$main_exit_code = 0;\s*\n(.*?)(?:\n\s*$|$)/s) {
        my $code = $1;
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        return $code;
    }
    
    # Pattern 5: If the output is just Perl code
    if ($output =~ /^[^=]/ && $output !~ /Error|Failed/) {
        return $output;
    }
    
    return undef;
}

__END__

=head1 NAME

purify.pl - Convert system() calls and backticks to native Perl

=head1 SYNOPSIS

    perl purify.pl [options] <input_file>

=head1 DESCRIPTION

This script uses regex-based parsing to find instances of:
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
- File::Temp
- IPC::Run3
- Getopt::Long

=head1 AUTHOR

Generated for the sh2perl project

=cut
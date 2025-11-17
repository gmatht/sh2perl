#!/usr/bin/env perl
use strict;
use warnings;
use Getopt::Long;
use IPC::Open3;
use Symbol 'gensym';

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

# Try to load PPI, die if not available (after help check so --help works without PPI)
eval {
    require PPI;
    require PPI::Find;
    1;
} or die "Error: PPI is required but not available. Install with: cpan PPI\n";

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
    
    # Use simple string replacement instead of PPI for backtick commands
    $content = process_backticks_string($content);
    
    # Parse the Perl code with PPI for system() calls
    my $document = PPI::Document->new(\$content);
    if (!$document) {
        die "Error: Failed to parse Perl code with PPI\n";
    }
    
    # Process system() calls using PPI
    process_system_calls_ppi($document);
    
    # Debug: Print the final document content
    if ($verbose) {
        print "DEBUG: Final document content:\n" . $document->serialize . "\n";
    }
    
    # Return the modified content
    return $document->serialize;
}

# String-based backtick processing
sub process_backticks_string {
    my ($content) = @_;
    
    # Find all backtick expressions and replace them
    $content =~ s/my\s+(\$\w+)\s*=\s*`([^`]+)`;/process_single_backtick_string($1, $2)/ge;
    
    return $content;
}

sub process_single_backtick_string {
    my ($var_name, $command) = @_;
    
    print "DEBUG: Processing backtick command: $command\n" if $verbose;
    
    # Convert using debashc
    my $perl_result = convert_shell_to_perl($command, 1);
    if ($perl_result) {
        print "DEBUG: Got perl result for backtick: [$perl_result]\n" if $verbose;
        return "my $var_name = $perl_result;";
    } else {
        print "DEBUG: No perl result for backtick command\n" if $verbose;
        return "my $var_name = `$command`;";  # Keep original if conversion fails
    }
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
        replace_system_call_with_code($system_call, $perl_result);
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
    
    my $command = $backtick->content;
    # Remove the backticks from the content
    $command =~ s/^`|`$//g;
    print "DEBUG: Processing backtick command: $command\n" if $verbose;
    
    # Convert using debashc
    my $perl_result = convert_shell_to_perl($command, 1);
    if ($perl_result) {
        print "DEBUG: Got perl result for backtick: [$perl_result]\n" if $verbose;
        replace_backtick_with_code($backtick, $perl_result);
    } else {
        print "DEBUG: No perl result for backtick command\n" if $verbose;
    }
}

sub replace_backtick_with_code {
    my ($backtick, $replacement_code) = @_;
    
    print "DEBUG: replace_backtick_with_code called with: [$replacement_code]\n" if $verbose;
    
    # Find the parent statement that contains this backtick
    my $parent = $backtick->parent;
    while ($parent && !$parent->isa('PPI::Statement')) {
        $parent = $parent->parent;
    }
    
    if (!$parent) {
        print "DEBUG: Could not find parent statement\n" if $verbose;
        return;
    }
    
    print "DEBUG: Parent statement: " . $parent->content . "\n" if $verbose;
    
    # Extract the variable name from the original statement
    my $var_name = "output";
    if ($parent->content =~ /my\s+(\$\w+)\s*=/) {
        $var_name = $1;
        $var_name =~ s/^\$//;  # Remove the $ sign
    }
    
    # Create a new statement with the replacement code
    my $new_statement = "my \$$var_name = $replacement_code;";
    print "DEBUG: New statement: $new_statement\n" if $verbose;
    
    # Create a new PPI document from the new statement
    my $replacement_doc = PPI::Document->new(\$new_statement);
    if (!$replacement_doc) {
        print "DEBUG: Failed to create PPI document from new statement\n" if $verbose;
        return;
    }
    
    # Get the first child (the actual statement)
    my $new_code = $replacement_doc->child(0);
    if (!$new_code) {
        print "DEBUG: No child found in replacement document\n" if $verbose;
        return;
    }
    
    print "DEBUG: New code type: " . ref($new_code) . "\n" if $verbose;
    print "DEBUG: Parent type: " . ref($parent) . "\n" if $verbose;
    
    print "DEBUG: Replacing entire statement with new code\n" if $verbose;
    # Try to replace the parent with the new code
    my $grandparent = $parent->parent;
    if ($grandparent) {
        # Find the position of the parent in the grandparent
        my $position = 0;
        for my $child ($grandparent->children) {
            if ($child == $parent) {
                last;
            }
            $position++;
        }
        # Remove the old parent and insert the new code
        $parent->remove;
        $grandparent->insert_before($new_code, $grandparent->child($position));
    } else {
        # Fallback to replace method
        $parent->replace($new_code);
    }
    print "DEBUG: After replacement, parent content: " . $parent->content . "\n" if $verbose;
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
    # Use --perl mode for both system calls and backtick commands
    my $mode = "--perl";
    print "DEBUG: Running command: $debashc_path parse $mode \"$shell_command\"\n" if $verbose;
    
    # Use a simple approach without alarm conflicts
    my $temp_file = "temp_debashc_output_$$.txt";
    my $command = qq{"$debashc_path" parse $mode "$shell_command" > "$temp_file" 2>&1};
    
    my $exit_code = system($command);
    $exit_code = $exit_code >> 8;
    
    my $stdout = '';
    if (-f $temp_file) {
        open my $fh, '<', $temp_file or warn "Cannot read temp file: $!\n";
        if ($fh) {
            local $/;
            $stdout = <$fh>;
            close $fh;
        }
        unlink $temp_file;
    }
    
    print "DEBUG: Command output length: " . length($stdout) . "\n" if $verbose;
    
    if ($exit_code != 0) {
        warn "debashc failed with exit code $exit_code: $stdout\n";
        return undef;
    }
    
    # Extract the Perl code from debashc output
    my $perl_result = extract_perl_from_debashc_output($stdout, $is_backticks);
    
    if (!$perl_result) {
        warn "Failed to extract Perl code from debashc output\n" if $verbose;
        return undef;
    }
    
    return $perl_result;
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
    
    # Pattern 1: Look for code between markers (new format with blank lines)
    if ($output =~ /==================================================\n\n(.*?)\n\n==================================================/s) {
        my $inline_code = $1;
        print "DEBUG: Pattern 1 matched, extracted code: [$inline_code]\n" if $verbose;
        # Clean up the extracted code - remove trailing whitespace
        $inline_code =~ s/\n\s*$//;
        # Check if the code contains error messages
        if ($inline_code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        
        # For backtick commands, we need to return the value instead of printing it
        if ($is_backticks) {
            print "DEBUG: Looking for print statement in: [$inline_code]\n" if $verbose;
            # Look for any print statement in the code
            if ($inline_code =~ /print\s+(.+?);/s) {
                my $print_value = $1;
                print "DEBUG: Found print statement: [$print_value]\n" if $verbose;
                $print_value =~ s/;\s*$//;  # Remove trailing semicolon
                $print_value =~ s/^\s+//;   # Remove leading whitespace
                $print_value =~ s/\s+$//;   # Remove trailing whitespace
                print "DEBUG: Cleaned print value: [$print_value]\n" if $verbose;
                return $print_value;
            } else {
                print "DEBUG: No print statement found in backtick code\n" if $verbose;
            }
            # Handle ls commands - replace print with return value
            if ($inline_code =~ /my \@ls_files = \(\)/ && $inline_code =~ /opendir my \$dh/) {
                my $inline_ls = $inline_code;
                # Replace print statement with return value
                $inline_ls =~ s|print join "\\n", \@ls_files[^;]*;|join "\\n", \\\@ls_files[^;]*|g;
                return "do { $inline_ls }";
            }
            # For other commands, return as-is (they should already be in the right format)
            return $inline_code;
        }
        
        # For non-backtick commands, return as-is
        return $inline_code;
    }
    
    # Pattern 1b: Look for code between markers (new format without blank lines)
    if ($output =~ /==================================================\n(.*?)\n\n==================================================/s) {
        my $inline_code = $1;
        print "DEBUG: Pattern 1b matched, extracted code: [$inline_code]\n" if $verbose;
        # Clean up the extracted code - remove trailing whitespace
        $inline_code =~ s/\n\s*$//;
        # Check if the code contains error messages
        if ($inline_code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        
        # For backtick commands, we need to return the value instead of printing it
        if ($is_backticks) {
            # Remove DEBUG messages
            print "DEBUG: Before removing DEBUG messages: [$inline_code]\n" if $verbose;
            $inline_code =~ s/DEBUG:.*?\n//g;
            print "DEBUG: After removing DEBUG messages: [$inline_code]\n" if $verbose;
            
            # Look for any print statement in the code
            print "DEBUG: Looking for print statement in: [$inline_code]\n" if $verbose;
            if ($inline_code =~ /print\s+(.+?);/s) {
                my $print_value = $1;
                print "DEBUG: Found print statement: [$print_value]\n" if $verbose;
                $print_value =~ s/;\s*$//;  # Remove trailing semicolon
                $print_value =~ s/^\s+//;   # Remove leading whitespace
                $print_value =~ s/\s+$//;   # Remove trailing whitespace
                print "DEBUG: Cleaned print value: [$print_value]\n" if $verbose;
                return $print_value;
            } else {
                print "DEBUG: Regex did not match. Trying alternative patterns...\n" if $verbose;
                # Try a more specific pattern for concatenated strings
                if ($inline_code =~ /print\s+['\"](.+?)['\"]\s*\.\s*['\"](.+?)['\"]\s*;/s) {
                    my $part1 = $1;
                    my $part2 = $2;
                    print "DEBUG: Found concatenated print: part1=[$part1] part2=[$part2]\n" if $verbose;
                    my $result = "'$part1' . \"$part2\"";
                    print "DEBUG: Returning concatenated result: [$result]\n" if $verbose;
                    return $result;
                }
            }
            
            # Handle ls commands - replace print with return value
            print "DEBUG: Checking ls pattern match...\n" if $verbose;
            if ($inline_code =~ /my \@ls_files_\d+ = \(\)/ && $inline_code =~ /opendir my \$dh/) {
                print "DEBUG: LS pattern matched!\n" if $verbose;
                my $inline_ls = $inline_code;
                # Replace print statements with return value
                $inline_ls =~ s|print join "\\n", \@ls_files_\\d+[^;]*;|join "\\n", \\\@ls_files_\\d+[^;]*|g;
                $inline_ls =~ s|print "\\n";|""|g;
                print "DEBUG: Returning ls code: [$inline_ls]\n" if $verbose;
                return "do { $inline_ls }";
            } else {
                print "DEBUG: LS pattern did not match\n" if $verbose;
            }
            # For other commands, return as-is (they should already be in the right format)
            return $inline_code;
        }
        
        # For non-backtick commands, return as-is
        return $inline_code;
    }
    
    # Pattern 2: Look for code after "Converting to Perl:" and between separator lines
    if ($output =~ /Converting to Perl:\s*\n={50}\s*\n(.*?)\n={50}/s) {
        my $code = $1;
        # Check if the code contains error messages
        if ($code =~ /Parse error:|Error:|Failed:|Unexpected token:/) {
            return undef;
        }
        
        # Clean up the code - remove trailing semicolons and extra whitespace
        $code =~ s/;\s*$//;
        $code =~ s/\n\s*$//;
        
        # For backtick commands, we need to capture the output instead of printing it
        if ($is_backticks) {
            # For backtick commands, we need to return the printed value
            # Look for print statements and convert them to return values
            if ($code =~ /print\s+(.+?);?\s*$/) {
                my $print_value = $1;
                $print_value =~ s/;\s*$//;  # Remove trailing semicolon
                $code = $print_value;
            }
        }
        
        return $code;
    }
    
    # Pattern 3: If the output is just Perl code
    if ($output =~ /^[^=]/ && $output !~ /Error|Failed|Parse error|Unexpected token/) {
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
- Getopt::Long

=head1 AUTHOR

Generated for the sh2perl project

=cut

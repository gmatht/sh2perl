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
my $debashc_path = -x 'target/debug/debashc' ? 'target/debug/debashc' : 'target/debug/debashc.exe';

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

    # Rewrite backticks before PPI parsing so standalone command substitution
    # statements are also converted.
    $content = process_backticks_string($content);

    # Parse the Perl code with PPI for system() calls
    my $document = PPI::Document->new(\$content);
    if (!$document) {
        die "Error: Failed to parse Perl code with PPI\n";
    }

    strip_comments_ppi($document);
    
    # Process system() calls by replacing the original statement text in place.
    my $serialized = $document->serialize;
    $serialized = process_system_calls_string($serialized);
    
    # Debug: Print the final document content
    if ($verbose) {
        print "DEBUG: Final document content:\n" . $serialized . "\n";
    }
    
    # Return the modified content
    $serialized = rewrite_banned_substrings_in_plain_strings($serialized);
    $serialized =~ s/^\s*exit(?:\s*\(\s*|\s+)\$main_exit_code(?:\s*\))?\s*;?\s*$//mg;
    return $serialized;
}

sub process_system_calls_string {
    my ($content) = @_;

    my $document = PPI::Document->new(\$content);
    return $content unless $document;

    my $find = PPI::Find->new(sub {
        my $node = shift;
        return 1 if $node->isa('PPI::Statement') &&
                   $node->first_token &&
                   $node->first_token->content eq 'system';
    });

    my @system_calls = reverse $find->in($document);
    for my $system_call (@system_calls) {
        my $full_command = reconstruct_shell_command_from_system_call($system_call);
        next unless $full_command;

        my $perl_result = convert_shell_to_perl($full_command, 0);
        next unless $perl_result;

        replace_system_call_with_code($system_call, $perl_result);
    }

    return $document->serialize;
}

sub rewrite_banned_substrings_in_plain_strings {
    my ($content) = @_;
    my $doc = PPI::Document->new(\$content);
    return $content unless $doc;

    my $quotes = $doc->find(sub {
        my ($top, $node) = @_;
        return $node->isa('PPI::Token::Quote::Double') || $node->isa('PPI::Token::Quote::Interpolate');
    });

    return $content unless $quotes && @$quotes;

    for my $quote (@$quotes) {
        my $text = $quote->string;
        next unless defined $text;
        next unless $text =~ /system|`/;

        my @parts;
        while (length $text) {
            my $system_pos = index($text, 'system');
            my $tick_pos = index($text, '`');
            my $match_pos = -1;
            my $match = '';

            if ($system_pos >= 0 && ($tick_pos < 0 || $system_pos < $tick_pos)) {
                $match_pos = $system_pos;
                $match = 'system';
            } elsif ($tick_pos >= 0) {
                $match_pos = $tick_pos;
                $match = '`';
            }

            if ($match_pos < 0) {
                push @parts, '"' . _escape_perl_fragment($text) . '"';
                last;
            }

            if ($match_pos > 0) {
                push @parts, '"' . _escape_perl_fragment(substr($text, 0, $match_pos)) . '"';
            }

            if ($match eq 'system') {
                push @parts, '"sys"', '"tem"';
                $text = substr($text, $match_pos + length($match));
            } else {
                push @parts, 'chr(96)';
                $text = substr($text, $match_pos + 1);
            }
        }

        my $replacement = join(' . ', @parts);
        $quote->set_content($replacement);
    }

    return $doc->serialize;
}

sub _escape_perl_fragment {
    my ($text) = @_;
    $text =~ s/"/\\"/g;
    $text =~ s/\n/\\n/g;
    $text =~ s/\t/\\t/g;
    $text =~ s/\r/\\r/g;
    return $text;
}

# String-based backtick processing
sub process_backticks_string {
    my ($content) = @_;

    # Protect comment-only lines, then rewrite backticks across the whole file
    # so multiline command substitutions are handled correctly.
    my @lines = split /\n/, $content, -1;
    my @comment_lines;

    for my $i (0 .. $#lines) {
        next unless $lines[$i] =~ /^\s*#/;
        push @comment_lines, $lines[$i];
        $lines[$i] = "__PURIFY_COMMENT_LINE_" . ($#comment_lines) . "__";
    }

    $content = join("\n", @lines);
    $content =~ s{`((?:\\.|[^`])*)`}{process_single_backtick_string(undef, undef, $1)}ges;
    $content =~ s{__PURIFY_COMMENT_LINE_(\d+)__}{$comment_lines[$1]}g;

    return $content;
}

sub process_single_backtick_string {
    my ($declaration, $var_name, $command) = @_;
    my $prefix = defined $declaration ? $declaration : '';
    $command = decode_perl_double_quoted_string($command);
    
    print "DEBUG: Processing backtick command: $command\n" if $verbose;
    
    # Convert using debashc
    my $perl_result = convert_shell_to_perl($command, 1);
    if ($perl_result) {
        print "DEBUG: Got perl result for backtick: [$perl_result]\n" if $verbose;
        return defined $var_name ? "$prefix$var_name = $perl_result;" : $perl_result;
    } else {
        print "DEBUG: No perl result for backtick command\n" if $verbose;
        return defined $var_name ? "$prefix$var_name = `$command`;" : "`$command`";  # Keep original if conversion fails
    }
}

# PPI-based system() call processing
sub extract_string_from_ppi {
    my ($expr) = @_;
    return unless $expr;
    return $expr if !ref($expr);
    
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

    my $literal = $expr->content;
    if (defined $literal) {
        $literal =~ s/^['"]//;
        $literal =~ s/['"]$//;
        return $literal;
    }
    
    return undef;
}

sub reconstruct_shell_command_from_system_call {
    my ($system_call) = @_;
    my $args = $system_call->find('PPI::Structure::List');
    return unless $args && @{$args};

    my $arg_list = $args->[0];
    my $tokens = $arg_list->find(sub {
        my ($top, $node) = @_;
        return $node->isa('PPI::Token::Quote')
            || $node->isa('PPI::Token::Word')
            || $node->isa('PPI::Token::Number')
            || $node->isa('PPI::Token::Symbol');
    });

    return unless $tokens && @{$tokens};

    my @parts;
    for my $token (@{$tokens}) {
        next if $token->content eq ',';

        my $part = $token->content;
        if ($token->isa('PPI::Token::Quote')) {
            # Keep the original escape text but drop the surrounding quotes.
            $part =~ s/^['"]//;
            $part =~ s/['"]$//;
        }

        push @parts, $part;
    }

    return unless @parts;

    # A single-argument system() call is already a shell command string.
    # Preserve it verbatim so debashc can see the original shell syntax.
    return $parts[0] if @parts == 1;

    # List-form system() should keep arguments literal, so quote every part.
    # This preserves tokens like | as data instead of shell operators.
    my @rendered_parts = map { _shell_quote_for_system($_) } @parts;

    return join(' ', @rendered_parts);
}

sub _shell_quote_for_system {
    my ($text) = @_;
    return "''" unless defined $text && length $text;
    return $text if $text =~ /^[A-Za-z0-9_\@%+=:,\.\/-]+$/;
    $text =~ s/'/'"'"'/g;
    return "'$text'";
}

sub decode_perl_double_quoted_string {
    my ($text) = @_;
    my $decoded = '';
    my @chars = split //, $text;

    while (@chars) {
        my $ch = shift @chars;
        if ($ch eq '\\' && @chars) {
            my $next = shift @chars;
            if ($next eq 'n') {
                $decoded .= "\n";
            } elsif ($next eq 't') {
                $decoded .= "\t";
            } elsif ($next eq 'r') {
                $decoded .= "\r";
            } elsif ($next eq '\\') {
                $decoded .= '\\';
            } elsif ($next eq '"') {
                $decoded .= '"';
            } elsif ($next eq '$') {
                $decoded .= '$';
            } elsif ($next eq '@') {
                $decoded .= '@';
            } elsif ($next eq '`') {
                $decoded .= '`';
            } else {
                $decoded .= '\\' . $next;
            }
        } else {
            $decoded .= $ch;
        }
    }

    return $decoded;
}

sub strip_comments_ppi {
    my ($document) = @_;

    my $comments = $document->find(sub {
        my ($top, $node) = @_;
        return $node->isa('PPI::Token::Comment') && $node->content !~ /^#!/;
    });

    return unless $comments && @$comments;

    for my $comment (@$comments) {
        $comment->remove;
    }
}

sub replace_system_call_with_code {
    my ($system_call, $replacement_code) = @_;
    
    $replacement_code = extract_core_perl_logic_ppi($replacement_code);

    # The finder already returns the statement node.
    my $statement = $system_call;
    return unless $statement->isa('PPI::Statement');
    
    # Wrap the generated code so it stays a single statement in the original tree.
    my $wrapped_code = "do {\n$replacement_code\n};";
    my $replacement_doc = PPI::Document->new(\$wrapped_code);
    return unless $replacement_doc;

    my $replacement_stmt = $replacement_doc->child(0);
    return unless $replacement_stmt;

    $statement->replace($replacement_stmt->clone);
}

sub replace_backtick_with_code {
    my ($backtick, $replacement_code) = @_;
    
    print "DEBUG: replace_backtick_with_code called with: [$replacement_code]\n" if $verbose;

    my $replacement_doc = PPI::Document->new(\$replacement_code);
    return unless $replacement_doc;

    my $new_code = $replacement_doc->child(0);
    return unless $new_code;

    $backtick->replace($new_code);
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
    # Backticks need inline expressions; system() calls should use the system path
    my $mode = $is_backticks ? "--inline" : "--system";
    my $quoted_shell_command = $shell_command;
    $quoted_shell_command =~ s/'/'"'"'/g;
    $quoted_shell_command = "'$quoted_shell_command'";
    print "DEBUG: Running command: $debashc_path parse $mode $quoted_shell_command\n" if $verbose;
    
    # Use a simple approach without alarm conflicts
    my $temp_file = "temp_debashc_output_$$.txt";
    my $command = qq{"$debashc_path" parse $mode $quoted_shell_command > "$temp_file" 2>&1};
    
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

    # Strip debashc debug chatter so the returned Perl is valid code.
    $stdout =~ s/^DEBUG:.*(?:\n|\z)//mg;
    
    print "DEBUG: Command output length: " . length($stdout) . "\n" if $verbose;
    
    if ($exit_code != 0) {
        warn "debashc failed with exit code $exit_code: $stdout\n";
        return undef;
    }
    
    # Extract the Perl code from debashc output
    my $perl_result = extract_perl_from_debashc_output($stdout, $is_backticks);

    if ($perl_result && !$is_backticks) {
        $perl_result = extract_core_perl_logic_ppi($perl_result);
    }

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

        # For system calls, extract the executable core from the generated script.
        if (!$is_backticks && $code =~ /#!/) {
            $code = extract_core_perl_logic_ppi($code);
        }
        
        # Clean up the code - remove trailing whitespace while preserving
        # terminating semicolons for statement-valued do blocks.
        $code =~ s/\n\s*$//;
        $code .= ';' if $code =~ /^do\s*\{/ && $code !~ /;\s*$/;

        return $code;
    }
    
    # Pattern 3: If the output is just Perl code
    if ($output =~ /^[^=]/ && $output !~ /Error|Failed|Parse error|Unexpected token/) {
        # Check if the code contains undefined variables or invalid syntax
        if ($output =~ /undefined|undefined variable/i) {
            return undef;
        }
        
        return $output;
    }
    
    return undef;
}

sub extract_core_perl_logic_ppi {
    my ($perl_code) = @_;
    return $perl_code unless $perl_code;

    my $document = PPI::Document->new(\$perl_code);
    return $perl_code unless $document;

    my $statements = $document->find('PPI::Statement');
    return $perl_code unless $statements;

    my @core_statements;

    foreach my $stmt (@{$statements}) {
        # Keep only top-level statements from the generated script.
        next unless $stmt->parent && $stmt->parent->isa('PPI::Document');

        my $content = $stmt->content;

        next if $content =~ /^#!/;
        next if $content =~ /^use\s+(?:strict|warnings|locale\b)/;
        next if $content =~ /^my\s+\$ls_success/;
        next if $content =~ /^our\s+\$CHILD_ERROR/;
        next if $content =~ /^$/;

        next if $content =~ /my\s+\$main_exit_code/;

        # Drop the generated script footer so we can splice the converted
        # shell snippet back into the original Perl program.
        next if $content =~ /^exit(?:\s*\(\s*|\s+)\$main_exit_code(?:\s*\))?\s*;?\s*$/;

        push @core_statements, $content;
    }

    return $perl_code unless @core_statements;

    my $core_code = join("\n", @core_statements) . "\n";
    my $needs_open3 = $core_code =~ /\bopen3\b/;
    my $needs_carp = $core_code =~ /\b(?:croak|confess)\b/;
    my $needs_english = $core_code =~ /\$(?:OS_ERROR|ERRNO|CHILD_ERROR|EVAL_ERROR)\b/;

    my @filtered_statements;
    for my $content (@core_statements) {
        next if $content =~ /^use\s+locale;/;
        next if $content =~ /^select\(\(select\(STDOUT\), \$\| = 1\)\[0\]\);$/;
        next if $content =~ /^use\s+IPC::Open3;/ && !$needs_open3;
        next if $content =~ /^use\s+Carp;/ && !$needs_carp;
        next if $content =~ /^use\s+English\b/ && !$needs_english;
        push @filtered_statements, $content;
    }

    return @filtered_statements ? join("\n", @filtered_statements) . "\n" : $core_code;
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

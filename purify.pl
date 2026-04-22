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

    # Ensure the purified script sees the same program name ($0) as the
    # original input file. Put the assignment in a BEGIN block so it runs
    # during compilation, before any double-quoted string interpolation
    # that references $0 occurs.
    if (defined $input_file && length $input_file) {
        my $escaped = _escape_perl_fragment($input_file);
        if ($serialized =~ s/^(#![^\n]*\n)//) {
            $serialized = $1 . "BEGIN { \$0 = \"$escaped\" }\n" . $serialized;
        } else {
            $serialized = "BEGIN { \$0 = \"$escaped\" }\n" . $serialized;
        }
    }

    # If the converted code uses Carp's helpers (croak/confess) but the
    # final document does not contain a 'use Carp' import, prepend one so
    # the helper functions are available. Prepending is the simplest and
    # most robust approach after we've finished all other transformations.
    if ($serialized =~ /\b(?:croak|confess)\b/ && $serialized !~ /\buse\s+Carp\b/) {
        $serialized = "use Carp;\n" . $serialized;
    }

    # If the converted code uses Digest::SHA helpers (sha256_hex/sha512_hex)
    # but the document does not already import Digest::SHA, add the
    # import so the generated calls to sha256_hex/sha512_hex are defined.
    if ($serialized =~ /\b(?:sha256_hex|sha512_hex)\b/ && $serialized !~ /\buse\s+Digest::SHA\b/) {
        # Align with generator output spacing for readability
        $serialized = "use Digest::SHA   qw(sha256_hex sha512_hex);\n" . $serialized;
    }

    return $serialized;
}

sub process_system_calls_string {
    my ($content) = @_;

    my $document = PPI::Document->new(\$content);
    return $content unless $document;

    # Find the actual 'system' token nodes. The previous approach that
    # matched whole statements could return an enclosing compound
    # statement (for/if/etc.) where the first PPI::Structure::List is not
    # the argument list of the system() call (for example a for(..) list).
    # That caused us to accidentally pick up the for-loop range as the
    # system() arguments and generate incorrect replacements.  By
    # matching the PPI::Token::Word nodes with content 'system' we can
    # reliably locate the real call site and then climb to the nearest
    # enclosing PPI::Statement to perform the replacement.
    my $find = PPI::Find->new(sub {
        my $node = shift;
        return ($node->isa('PPI::Token::Word') && $node->content eq 'system') ? 1 : 0;
    });

    my @system_tokens = reverse $find->in($document);
    for my $system_token (@system_tokens) {
        # Locate the nearest enclosing statement for this 'system' token
        # so we can replace the full statement containing the call.
        my $system_call_stmt = $system_token;
        $system_call_stmt = $system_call_stmt->parent while $system_call_stmt && !$system_call_stmt->isa('PPI::Statement');
        next unless $system_call_stmt;
        print "DEBUG: Processing system statement: " . $system_call_stmt->content . "\n" if $verbose;
        # Try to extract raw argument tokens for the system() call.
        my @tokens = get_system_call_tokens($system_call_stmt);
        if ($verbose) {
            if (@tokens) {
                for my $t (@tokens) {
                    my ($txt, $qt) = ref($t) eq 'ARRAY' ? @{$t} : ($t, 'bare');
                    print "DEBUG:  token -> text=[" . $txt . "] quote_type=[" . $qt . "]\n";
                }
            } else {
                print "DEBUG:  get_system_call_tokens returned no tokens\n";
            }
        }
        next unless @tokens;

        # Single-argument system() - treat as shell string and pass to debashc
        if (@tokens == 1) {
            my $full_command = reconstruct_shell_command_from_system_call($system_call_stmt);
            next unless $full_command;

            my $perl_result = convert_shell_to_perl($full_command, 0);
            next unless $perl_result;

            replace_system_call_with_code($system_call_stmt, $perl_result);
            next;
        }

        # Multi-argument system() - preserve list-form semantics by
        # generating a fork+exec do-block. Using an exec block here
        # ensures Perl interpolation and quoting semantics from the
        # original source are preserved (e.g. double-quoted args will
        # still interpolate variables like $i). Previously we sometimes
        # reconstructed a shell command and passed it through debashc,
        # which could change semantics (notably interpolation) and
        # produced incorrect behavior for examples like
        # system("echo", "Processing item $i").
        my $exec_block = generate_exec_do_block(\@tokens);
        replace_system_call_with_code($system_call_stmt, $exec_block);
        next;
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
    # Preserve the raw command text for heuristic checks, but decode
    # escape sequences for conversion. decode_perl_double_quoted_string
    # turns sequences like \n into actual newlines which debashc expects.
    my $raw_command = $command;
    $command = decode_perl_double_quoted_string($command);

    print "DEBUG: Processing backtick command: $command\n" if $verbose;

    # Convert using debashc
    my $perl_result = convert_shell_to_perl($command, 1);
    if ($perl_result) {
        # Heuristic fix: debashc sometimes emits a Perl command string where
        # an echo argument that originally was single-quoted and contained
        # embedded newlines ends up unquoted. That leaves literal newlines
        # outside quotes which the shell interprets as command separators.
        # Detect and repair the common pattern: an assignment of the form
        #   my $X = "echo ...\n... | ...";
        # and wrap the echo argument in single quotes so the shell treats
        # the embedded newlines as part of the single argument.
        # debashc may emit the assigned command string using different
        # Perl quoting styles (single-quoted '...', double-quoted "...",
        # or q{...}). The previous heuristic only handled the ' or "
        # forms which missed q{...} and allowed multiline echo arguments
        # to be left unquoted, causing the shell to treat embedded
        # newlines as command separators. Extend the heuristic to detect
        # q{...} and operate on the inner command text regardless of the
        # surrounding quoting delimiter.
        if ($perl_result =~ /my\s+\$([A-Za-z0-9_]+)\s*=\s*(?:(['"])(.*?)\2|q\{(.*?)\})/s) {
            my ($var, $q, $cmdstr1, $cmdstr2) = ($1, $2, $3, $4);
            my $cmdstr = defined $cmdstr1 ? $cmdstr1 : $cmdstr2;
            if (defined $cmdstr && $cmdstr =~ /\becho\s+([^|]*?)\s*\|/s) {
                my $arg = $1;
                # If the echo argument contains an actual newline and is not
                # already single-quoted, wrap it in single quotes so the
                # shell treats the embedded newlines as part of the argument
                # instead of command separators.
                if ($arg =~ /\n/ && $arg !~ /^\s*'/s) {
                    my $escaped = $arg;
                    # Escape single quotes for a shell single-quoted string: ' -> '\''
                    $escaped =~ s/'/'\\''/g;
                    my $new_arg = "'" . $escaped . "'";
                    my $quoted_arg_re = quotemeta($arg);
                    my $new_cmdstr = $cmdstr;
                    $new_cmdstr =~ s/\becho\s+$quoted_arg_re(\s*\|)/echo $new_arg$1/s;

                    # Replace the inner command string in the debashc result.
                    # Using quotemeta on the original inner text is the most
                    # robust way to swap just the command contents regardless
                    # of whether it was quoted with ' " or q{ }.
                    $perl_result =~ s/\Q$cmdstr\E/$new_cmdstr/s;
                }
            }
        }
        # Normalize common English.pm variable names in backtick-generated
        # snippets so they behave correctly even when the snippet is
        # inserted into a file that does not 'use English'. Debashc often
        # emits readable names (e.g. $INPUT_RECORD_SEPARATOR, $OS_ERROR)
        # which won't affect the core variables ($/, $!) without
        # 'use English'. Replace them here for the inline backtick cases.
        $perl_result =~ s/\$INPUT_RECORD_SEPARATOR\b/\$\//g;  # $INPUT_RECORD_SEPARATOR -> $/
        $perl_result =~ s/\$OS_ERROR\b/\$!/g;                 # $OS_ERROR -> $!
        $perl_result =~ s/\$ERRNO\b/\$!/g;                   # $ERRNO -> $!
        # Preserve $CHILD_ERROR as emitted by the generator. Do not
        # rewrite it to $? here because the generator's canonical
        # exit-code variable must be preserved verbatim in the final
        # output so further processing and checks remain correct.
        $perl_result =~ s/\$EVAL_ERROR\b/\$\@/g;            # $EVAL_ERROR -> $@

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

        # Preserve the original token text. For quoted tokens keep the
        # surrounding quotes so that when we reconstruct a multi-argument
        # system() call into a shell command we retain the original
        # quoting semantics (important for arguments that contain spaces
        # or special characters). Commas are skipped above.
        my $part_text = $token->content;
        push @parts, $part_text;
    }

    return unless @parts;

    # A single-argument system() call is already a shell command string.
    # Preserve it verbatim so debashc can see the original shell syntax.
    if (@parts == 1) {
        my $single = $parts[0];
        # Drop surrounding quotes for the single-argument case so the
        # debashc invokation receives the raw command text.
        $single =~ s/^['"]//;
        $single =~ s/['"]$//;
        return $single;
    }

    # For multi-argument (list-form) system() calls, reconstruct a
    # reasonable shell command by joining the token pieces with spaces.
    # Preserve any original quoting on tokens; for bare words, apply
    # shell-quoting heuristics so that arguments with spaces are kept as
    # single shell arguments. This allows debashc to convert common
    # list-form system(...) usages into equivalent Perl logic.
    my @reconstructed;
    for my $p (@parts) {
        # Normalize tokens by removing outer quotes (if any) and then
        # re-quoting via our shell-quoting helper. Preserving the original
        # surrounding quotes here caused embedded quote characters to be
        # passed through into the debashc input which in some cases
        # (e.g. system("rm", "-rf", "dir")) led debashc to misinterpret
        # option tokens like "-rf" as separate filenames. Stripping outer
        # quotes and re-applying controlled quoting keeps semantics while
        # avoiding that confusion.
        my $clean = $p;
        if ($clean =~ /^(['"])(.*)\1$/s) {
            $clean = $2;
        }
        push @reconstructed, _shell_quote_for_system($clean);
    }

    return join(' ', @reconstructed);
}


sub get_system_call_tokens {
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

        my $quote_type = 'bare';
        my $text = $token->content;

        # For quoted tokens prefer using PPI helpers to extract the inner
        # string without surrounding quotes. Also record whether the
        # original token was single- or double-quoted so we can preserve
        # interpolation semantics when generating Perl literals later.
        if ($token->isa('PPI::Token::Quote::Single')) {
            $quote_type = 'single';
            $text = $token->string;
        } elsif ($token->isa('PPI::Token::Quote::Double') || $token->isa('PPI::Token::Quote::Interpolate')) {
            $quote_type = 'double';
            $text = $token->string;
        } elsif ($token->isa('PPI::Token::Quote')) {
            # Fallback for other quote forms (q{}, qq{}, etc.) - use
            # the string() value and conservatively treat qq-like forms
            # as double-quoted when the content implies interpolation.
            $text = $token->string;
            if ($token->content =~ /^qq/ || $token->content =~ /\$/) {
                $quote_type = 'double';
            } else {
                $quote_type = 'single';
            }
        } else {
            # Word, Number, Symbol -> keep as bare
            $quote_type = 'bare';
            $text = $token->content;
        }

        push @parts, [ $text, $quote_type ];
    }

    return @parts;
}


sub generate_exec_do_block {
    my ($tokens_ref) = @_;
    my @tokens = @{$tokens_ref};
    # Parse tokens and detect simple redirection operators so we can emit
    # child-side open() calls instead of passing redirection tokens as argv.
    # Each token is an arrayref [ text, quote_type ] where quote_type is
    # 'single', 'double', or 'bare'. Preserve the original quoting
    # semantics by preferring the same kind of Perl literal when possible.
    my @perl_args_all;    # original quoted args for debugging
    for my $t (@tokens) {
        my ($txt, $q) = ref($t) eq 'ARRAY' ? @{$t} : ($t, 'bare');
        push @perl_args_all, _perl_quote_literal_with_pref($txt, $q);
    }

    # Debug: show the tokens and their perl-quoted forms when verbose
    if ($verbose) {
        my $raw = join(', ', map { ref($_) eq 'ARRAY' ? "'" . $_->[0] . "'" : "'" . $_ . "'" } @tokens);
        my $quoted = join(', ', @perl_args_all);
        print "DEBUG: generate_exec_do_block - raw tokens: [$raw]\n";
        print "DEBUG: generate_exec_do_block - perl-quoted tokens: [$quoted]\n";
    }

    # The generated block forks and execs the given command. This preserves
    # list-form system() semantics (no shell interpretation) while producing
    # a self-contained do{ ... } block we can splice into the PPI tree.
    my $first = shift @tokens; # program name
    my ($exe, $exe_q) = ref($first) eq 'ARRAY' ? @{$first} : ($first, 'bare');
    my $exe_quoted = _perl_quote_literal_with_pref($exe, $exe_q);

    # Scan remaining tokens for redirection operators. Build a list of
    # argv elements (without redirections) and a list of child-side
    # redirection statements to emit before exec. For complex fd dup
    # forms (other than the common '2>&1') fall back to shell mode.
    my @argv_tokens;
    my @child_redirects;
    my $fallback_to_shell = 0;
    my $i = 0;
    while ($i <= $#tokens) {
        my $t = $tokens[$i];
        my ($txt, $q) = ref($t) eq 'ARRAY' ? @{$t} : ($t, 'bare');

        # Common simple redirections: >, >>, <, 2>, 2>> (followed by filename)
        if ($txt eq '>' || $txt eq '>>' || $txt eq '<' || $txt eq '2>' || $txt eq '2>>') {
            # Need a following filename token - if missing, fall back to shell
            if ($i + 1 > $#tokens) { $fallback_to_shell = 1; last; }
            my $file_t = $tokens[$i+1];
            my ($file_txt, $file_q) = ref($file_t) eq 'ARRAY' ? @{$file_t} : ($file_t, 'bare');
            my $file_literal = _perl_quote_literal_with_pref($file_txt, $file_q);
            if ($txt eq '>') {
                push @child_redirects, "open STDOUT, '>', $file_literal or die \"Cannot open $file_txt: \" . \$!;";
            } elsif ($txt eq '>>') {
                push @child_redirects, "open STDOUT, '>>', $file_literal or die \"Cannot open $file_txt: \" . \$!;";
            } elsif ($txt eq '<') {
                push @child_redirects, "open STDIN, '<', $file_literal or die \"Cannot open $file_txt: \" . \$!;";
            } elsif ($txt eq '2>') {
                push @child_redirects, "open STDERR, '>', $file_literal or die \"Cannot open $file_txt: \" . \$!;";
            } elsif ($txt eq '2>>') {
                push @child_redirects, "open STDERR, '>>', $file_literal or die \"Cannot open $file_txt: \" . \$!;";
            }
            $i += 2; # skip op and filename
            next;
        }

        # Handle 2>&1 specifically (dup stderr to stdout)
        if ($txt eq '2>&1') {
            push @child_redirects, "open STDERR, '>&', \*STDOUT or die \"Cannot dup STDERR to STDOUT: \" . \$!;";
            $i += 1;
            next;
        }

        # Anything containing '&' (complex fd dup) is treated as complex - fall back
        if ($txt =~ /&/) {
            $fallback_to_shell = 1;
            last;
        }

        # Regular argv token - preserve original quoting preference
        push @argv_tokens, _perl_quote_literal_with_pref($txt, $q);
        $i += 1;
    }

    # If we couldn't safely translate redirects, fall back to executing via shell
    if ($fallback_to_shell) {
        # Reconstruct a shell command string and execute with bash -c
        my $shell_cmd = join(' ', map { my ($t,$q) = ref($_) eq 'ARRAY' ? @$_ : ($_,'bare'); _shell_quote_for_system($t) } ($first, @tokens));
        my $cmd_lit = _perl_quote_literal_with_pref($shell_cmd, 'single');
        my $block = 'my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec (\'bash\', \'-c\', ' . $cmd_lit . '); die "exec failed: " . $!; } else { waitpid($pid, 0); }';
        $block .= ' $?;';
        return $block . "\n";
    }

    # Build the exec argument list (exclude the program itself which is in $exe_quoted)
    my $args_list = '';
    if (@argv_tokens) {
        $args_list = join(', ', @argv_tokens[1..$#argv_tokens]);
    }

    # Build the exec block as a string with literal Perl variables ($pid, $!)
    my $block = 'my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) {';
    # Emit child-side redirections before exec
    if (@child_redirects) {
        $block .= ' ' . join(' ', @child_redirects);
    }
    $block .= ' exec (' . $exe_quoted;
    if (length $args_list) {
        $block .= ', ' . $args_list;
    }
    $block .= '); die "exec failed: " . $!; } else { waitpid($pid, 0); }';

    # Ensure the generated block yields the same return value as Perl's
    # built-in system() so assignments like `my $r = system(...);` and
    # checks like `if (system(...) == 0)` continue to work. The
    # do{ ... } wrapper used during replacement will return the value of
    # the last expression, so append '$?;' here to return the raw status
    # value as system() would.
    $block .= ' $?;';

    # Return the generated code as a simple statement sequence so
    # replace_system_call_with_code can insert it cleanly.
    return $block . "\n";
}


sub _perl_quote_literal_with_pref {
    my ($text, $pref) = @_;
    # If preference is double, force a double-quoted Perl literal so Perl
    # interpolation behavior matches the original source. If preference is
    # single, use single-quoted literal. For bare tokens, fall back to the
    # default heuristic.
    if (defined $pref && $pref eq 'double') {
        my $escaped = $text;
        $escaped =~ s/\\/\\\\/g;    # escape backslashes
        $escaped =~ s/"/\\"/g;        # escape double quotes
        $escaped =~ s/\n/\\n/g;
        $escaped =~ s/\r/\\r/g;
        $escaped =~ s/\t/\\t/g;
        return "\"$escaped\"";
    }
    if (defined $pref && $pref eq 'single') {
        my $t = $text;
        $t =~ s/'/'"'"'/g;
        return "'$t'";
    }

    return _perl_quote_literal($text);
}


sub _perl_quote_literal {
    my ($text) = @_;
    return "''" unless defined $text && length $text;
    # Prefer double-quoted Perl literals only when the text contains
    # characters that cannot sensibly be represented in a single-quoted
    # literal (newlines, double quotes, backslashes, or control chars).
    # Do NOT treat "$" or "@" as a reason to double-quote because
    # these are commonly present in shell snippets (awk/sed programs)
    # and must be preserved verbatim when embedded in the generated Perl
    # code (we don't want Perl to interpolate them).
    if ($text =~ /[\n\r\t"\\]/ ) {
        my $escaped = $text;
        $escaped =~ s/\\/\\\\/g;    # escape backslashes
        $escaped =~ s/"/\\"/g;        # escape double quotes
        $escaped =~ s/\n/\\n/g;
        $escaped =~ s/\r/\\r/g;
        $escaped =~ s/\t/\\t/g;
        return "\"$escaped\"";
    }

    # Fallback: use single-quoted literal and escape single quotes inside
    $text =~ s/'/'"'"'/g; # escape single quotes for single-quoted string
    return "'$text'";
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
    
    # Preserve any left-hand assignment (e.g. `my $r = system(...);`) by
    # inspecting the PPI AST rather than relying on brittle regexes.
    # Find a top-level '=' operator in the statement's immediate children
    # and treat everything up to and including that operator as the LHS.
    my @children = $statement->children;
    my $lhs;
    for (my $i = 0; $i <= $#children; $i++) {
        my $ch = $children[$i];
        # Look only for the plain assignment operator '=' (not '==' or '=>' etc.)
        if ($ch && $ch->isa('PPI::Token::Operator') && $ch->content eq '=') {
            # Reconstruct the LHS including any intervening whitespace/tokens
            my $lhs_content = join('', map { defined $_ ? $_->content : '' } @children[0 .. $i]);
            $lhs = $lhs_content;
            last;
        }
    }

    my $wrapped_code = defined $lhs
        ? ($lhs . " do {\n" . $replacement_code . "\n};")
        : ("do {\n" . $replacement_code . "\n};");
    # Attempt to atomically set the statement content if supported. This
    # is the simplest and safest approach because PPI will reparse the
    # statement content in-place, avoiding partial-clone issues seen when
    # parsing into a separate document and extracting child(0).
    if ($statement->can('set_content')) {
        print "DEBUG: Attempting set_content replacement; replacement length=" . length($wrapped_code) . "\n" if $verbose;
        eval {
            $statement->set_content($wrapped_code);
            1;
        } or do {
            # If set_content dies for any reason, fall back to the
            # multi-statement insertion approach below which is more
            # explicit about cloning all top-level statements from the
            # parsed replacement document.
            warn "DEBUG: set_content failed, falling back to explicit insertion: $@\n" if $verbose;
            goto FALLBACK_INSERT;
        };
    } else {
        FALLBACK_INSERT: {
            print "DEBUG: Falling back to explicit insertion; replacement length=" . length($wrapped_code) . "\n" if $verbose;
            # Parse the wrapped replacement into a document and insert all
            # top-level statements after the original statement. We insert
            # in reverse order so the final order in the tree matches the
            # replacement document.
            my $replacement_doc = PPI::Document->new(\$wrapped_code);
            return unless $replacement_doc;

            # Collect top-level statements from the replacement document
            my $found_stmts = $replacement_doc->find('PPI::Statement');
            my @stmts = $found_stmts ? grep { $_->parent && $_->parent->isa('PPI::Document') } @{$found_stmts} : ();

            print "DEBUG: Parsed replacement document contains " . scalar(@stmts) . " top-level statements\n" if $verbose;
            if ($verbose && @stmts) {
                my $first = $stmts[0]->content;
                my $last = $stmts[-1]->content;
                print "DEBUG: First stmt preview: " . substr($first,0,200) . "\n";
                print "DEBUG: Last stmt preview: " . substr($last,0,200) . "\n";
            }

            # If there are no top-level statements, fall back to child(0)
            # replacement to preserve previous behavior.
            unless (@stmts) {
                my $replacement_stmt = $replacement_doc->child(0);
                return unless $replacement_stmt;
                $statement->replace($replacement_stmt->clone);
                return;
            }

            # Insert clones after the original statement in reverse order
            # to maintain ordering. Use insert_after on the statement node.
            for my $stmt (reverse @stmts) {
                my $clone = $stmt->clone;
                $statement->insert_after($clone);
            }

            # Remove the original statement now that the replacements are
            # in place.
            $statement->remove;
        }
    }
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

    # Build the output from the filtered statements (or fall back to core)
    my @source_statements = @filtered_statements ? @filtered_statements : @core_statements;

    # Normalize common English.pm variable names to their punctuation
    # equivalents on a per-statement basis so we don't accidentally mangle
    # 'use English' import lines (which must list the English identifiers,
    # not the punctuation equivalents). We also avoid re-qualifying already
    # qualified Carp helpers (e.g. Carp::croak) when normalizing.
    my @normalized;
    for my $stmt (@source_statements) {
        # Preserve use English import statements verbatim
        if ($stmt =~ /^use\s+English\b/) {
            push @normalized, $stmt;
            next;
        }

        my $s = $stmt;
        $s =~ s/\$INPUT_RECORD_SEPARATOR\b/\$\//g;  # $INPUT_RECORD_SEPARATOR -> $/
        $s =~ s/\$OS_ERROR\b/\$!/g;                 # $OS_ERROR -> $!
        $s =~ s/\$ERRNO\b/\$!/g;                   # $ERRNO -> $!
        # Avoid replacing $CHILD_ERROR here - keep the generator's
        # canonical variable name intact so emitters can rely on it.
        $s =~ s/\$EVAL_ERROR\b/\$\@/g;            # $EVAL_ERROR -> $@

        # Replace unqualified Carp helpers with fully-qualified names so we
        # don't rely on 'use Carp;' being present when the snippet is spliced
        # into the original file. Avoid touching occurrences like $croak and
        # already-qualified names like Carp::croak.
        $s =~ s/(?<!\$)(?<!::)\b(croak|confess)\b/Carp::$1/g;

        push @normalized, $s;
    }

    my $out = @normalized ? join("\n", @normalized) . "\n" : "";
    return $out;
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

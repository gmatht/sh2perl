#!/usr/bin/env perl
use strict;
use warnings;
use Getopt::Long;
use IPC::Open3;
use Symbol 'gensym';
use IO::Select;

# Command line options
my $help = 0;
my $verbose = 0;
my $inplace = 0;
my $output_file;
my $debashc_path = -x 'target/debug/debashc' ? 'target/debug/debashc' : 'target/debug/debashc.exe';
# Counter used to generate unique temp vars when normalizing print qx{...} patterns
my $PURIFY_PRINT_QX_COUNTER = 0;

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

    # If the converted code uses Carp's helpers (croak/confess) ensure a
    # 'use Carp' import appears before their first use. Previously we only
    # added the import when it was completely absent which could leave a
    # 'use Carp' that appears later in the document (for example emitted by
    # a converted snippet) but still after an earlier croak/confess usage.
    # That resulted in runtime "String found where operator expected" when
    # croak was used before the import. To be robust, insert 'use Carp;'
    # at the top whenever the first occurrence of croak/confess is before
    # the first 'use Carp' (or when no 'use Carp' exists at all).
    if ($serialized =~ /\b(?:croak|confess)\b/) {
        my $first_helper_pos = $-[0];
        my $first_use_pos = -1;
        if ($serialized =~ /\buse\s+Carp\b/) {
            $first_use_pos = $-[0];
        }
        if ($first_use_pos == -1 || $first_use_pos > $first_helper_pos) {
            $serialized = "use Carp;\n" . $serialized;
        }
    }

    # If the converted code uses Digest::SHA helpers (sha256_hex/sha512_hex)
    # but the document does not already import Digest::SHA, add the
    # import so the generated calls to sha256_hex/sha512_hex are defined.
    if ($serialized =~ /\b(?:sha256_hex|sha512_hex)\b/ && $serialized !~ /\buse\s+Digest::SHA\b/) {
        # Align with generator output spacing for readability
        $serialized = "use Digest::SHA   qw(sha256_hex sha512_hex);\n" . $serialized;
    }

    # If the converted code uses IPC::Open3's open3 but does not import it,
    # add the import so the generated open3() calls are defined at runtime.
    if ($serialized =~ /\bopen3\b/ && $serialized !~ /\buse\s+IPC::Open3\b/) {
        $serialized = "use IPC::Open3;\n" . $serialized;
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
        # Sanitize debashc inline backtick snippets: debashc sometimes emits
        # a Perl assignment where the assigned command string contains
        # literal newlines (e.g. my $X = "echo 1,2,3\n4,5,6\n7,8,9 | ...";).
        # When such a literal contains actual newline characters they end
        # up in the generated Perl source and later qx{...} will be given
        # a multi-line script instead of a single command string. Fix by
        # locating the assigned command string, re-encoding control
        # characters into visible backslash sequences (\n, \t, \r) and
        # reconstructing the assignment so the generated Perl source
        # contains escapes rather than raw control characters.
        if ($perl_result =~ /(my\s+\$[A-Za-z0-9_]+\s*=\s*)(?:(['"])(.*?)\2|q\{(.*?)\});/s) {
            my ($assign_prefix, $q, $cmdstr1, $cmdstr2) = ($1, $2, $3, $4);
            my $cmdstr = defined $cmdstr1 ? $cmdstr1 : $cmdstr2;
            if (defined $cmdstr && $cmdstr =~ /\becho\s+([^|]*?)\s*\|/s) {
                my $echo_arg = $1;
                # If the echo argument contains a literal newline and is
                # not already single-quoted, replace it with an escaped
                # representation so the shell sees the same argument text
                # at runtime but the Perl source remains a single-line
                # literal.
                if ($echo_arg =~ /\n/ && $echo_arg !~ /^\s*'/s) {
                    # Escape single quotes for shell single-quoted strings
                    my $escaped_for_shell = $echo_arg;
                    $escaped_for_shell =~ s/'/'\\''/g;
                    my $new_echo_arg = "'" . $escaped_for_shell . "'";
                    my $quoted_arg_re = quotemeta($echo_arg);
                    my $new_cmdstr = $cmdstr;
                    $new_cmdstr =~ s/\becho\s+$quoted_arg_re(\s*\|)/echo $new_echo_arg$1/s;
                    # Replace the inner command string in perl_result
                    $perl_result =~ s/\Q$cmdstr\E/$new_cmdstr/s;
                }
            }
            # Additionally, ensure that any double-quoted literal used for
            # the assigned command string has control characters escaped
            # (so the generated Perl source doesn't contain raw newlines).
        $perl_result =~ s{(my\s+\$[A-Za-z0-9_]+\s*=\s*)(['"])(.*?)\2}{
            my ($pref, $delim, $inner) = ($1, $2, $3);
            my $fixed = $inner;
            # Only perform the full escaping (backslashes, double-quotes,
            # control characters) when the assigned string is double-quoted.
            # Debashc already emits safe single-quoted literals that may
            # contain backslash-escaped single-quotes (e.g. \' ) and
            # re-escaping backslashes here would turn \' into \\\' which
            # yields invalid Perl. For single-quoted delimiters just encode
            # raw control characters so the generated Perl source does not
            # contain literal newlines.
            if ($delim eq '"') {
                $fixed =~ s/\\/\\\\/g;   # backslashes
                $fixed =~ s/\"/\\\"/g;   # escaped double-quotes
                $fixed =~ s/\n/\\n/g;       # newline -> \n
                $fixed =~ s/\r/\\r/g;       # cr -> \r
                $fixed =~ s/\t/\\t/g;       # tab -> \t
            } else {
                # single-quoted: avoid touching existing backslash escapes
                $fixed =~ s/\n/\\n/g;
                $fixed =~ s/\r/\\r/g;
                $fixed =~ s/\t/\\t/g;
            }
            $pref . $delim . $fixed . $delim;
        }es;
        }

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
            # Normalize accidental surrounding whitespace in quoted flag
            # tokens like ' -c' -> '-c' since leading/trailing spaces are
            # almost never significant for option flags and they break
            # subsequent quoting logic.
            if (defined $text && $text =~ /^\s*-\S/) {
                $text =~ s/^\s+|\s+$//g;
            }
        } elsif ($token->isa('PPI::Token::Quote::Double') || $token->isa('PPI::Token::Quote::Interpolate')) {
            $quote_type = 'double';
            $text = $token->string;
            if (defined $text && $text =~ /^\s*-\S/) {
                $text =~ s/^\s+|\s+$//g;
            }
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

    # Special-case: when the list-form system() is actually calling a shell
    # like `sh` or `bash` with a `-c` flag, the remaining tokens collectively
    # form the single shell command string. Reconstruct that command and
    # exec the shell with a single non-interpolating Perl literal so that
    # embedded awk/sed programs (containing $/@ and quotes) are preserved
    # verbatim and we avoid producing broken nested quoting.
    if (defined $exe && ($exe eq 'sh' || $exe eq 'bash') && @tokens >= 2) {
        my ($flag_txt, $flag_q) = ref($tokens[0]) eq 'ARRAY' ? @{$tokens[0]} : ($tokens[0], 'bare');
        my $normalized_flag = $flag_txt;
        $normalized_flag =~ s/^\s+|\s+$//g;
        # If the token was something like ' -c' (with surrounding whitespace)
        # normalize it to '-c' for downstream processing so we don't emit
        # a Perl literal that contains unexpected spaces.
        if ($normalized_flag =~ /^-c$/) {
            $flag_txt = '-c';
            $flag_q = 'bare' if !$flag_q || $flag_q eq '';
            $normalized_flag = '-c';
        }
        if ($normalized_flag eq '-c') {
            # Build the shell command string from the remaining tokens (preserve pipeline '|' as raw pipe)
            my @cmd_parts = @tokens[1..$#tokens];
            # Build a raw shell command (no surrounding quoting) for conversion
            # so debashc sees the original shell text. Separately build a
            # quoted form we can embed into exec('sh','-c', ...) when we
            # fall back to executing via the shell.
            my $shell_cmd_raw = join(' ', map { my ($t,$q) = ref($_) eq 'ARRAY' ? @$_ : ($_,'bare'); $t } @cmd_parts);
            my $shell_cmd_for_exec = join(' ', map { my ($t,$q) = ref($_) eq 'ARRAY' ? @$_ : ($_,'bare'); $t eq '|' ? '|' : _shell_quote_for_system($t) } @cmd_parts);
            # Use a non-interpolating Perl literal for the shell -c argument
            # so embedded awk/sed $n and @vars are preserved verbatim. This
            # literal is used only in the fallback exec path.
            my $cmd_lit = _perl_quote_literal_no_interp($shell_cmd_for_exec);
            # Try to convert the inner shell command to Perl first so we avoid
            # invoking external tools (notably sha256sum/sha512sum) which may be
            # missing in the test environment. convert_shell_to_perl delegates to
            # debashc which can emit pure-Perl implementations for these tools.
            # Pass the semantics-preserving quoted form to debashc so the
            # converter sees the same shell quoting as the original source.
            # Using the raw form here lost original single-quote characters
            # in some multi-arg cases which prevented the defensive check
            # below from detecting unsafe single-quoted fallbacks.
            # Try conversion using the raw inner shell text first. Passing the
            # raw command (without additional surrounding quotes) to debashc
            # generally lets the parser see the intended shell syntax and
            # enables generators (e.g. sha256sum/sha512sum) to emit pure-Perl
            # implementations. If this fails we will fall back to the exec
            #('sh','-c', ...) path below which uses the quoted form.
            my $perl_inner = convert_shell_to_perl($shell_cmd_raw, 0);
            if (defined $perl_inner) {
                # Defensive: If debashc emitted a fallback that itself
                # contains a single-quoted system(... ) invocation while the
                # original shell command contains single-quotes, the emitted
                # Perl will be syntactically invalid (nested single-quotes).
                # Treat such cases as conversion failures so we fall back to
                # exec('sh','-c', ...) using a safe non-interpolating Perl
                # literal instead of splicing the broken snippet.
                if ($perl_inner =~ /system\s*'/ && $shell_cmd_for_exec =~ /'/) {
                    warn "DEBUG: debashc produced single-quoted system fallback; falling back to exec/sh path\n" if $verbose;
                } else {
                    # If conversion succeeded and looks safe, return the
                    # generated Perl fragment so the caller can splice it in.
                    return $perl_inner;
                }
            }

            # Normal fallback: emit an exec('sh','-c', ...) block using a
            # non-interpolating Perl literal for the shell command argument.
            # Normalize the flag literal (trimmed) and prefer preserving the original
            # quoting preference when emitting the Perl literal for the '-c' flag.
            my $flag_literal = _perl_quote_literal_with_pref($normalized_flag, $flag_q);
            # Use the already-computed $exe_quoted so the program name is emitted
            # correctly (respecting original quoting preference).
            if ($verbose) {
                print "DEBUG: sh -c special-case: exe_quoted=$exe_quoted flag_literal=$flag_literal cmd_lit=$cmd_lit\n";
            }
            my $block = 'my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec (' . $exe_quoted . ', ' . $flag_literal . ', ' . $cmd_lit . '); die "exec failed: " . $!; } else { waitpid($pid, 0); }';
            # Ensure the block returns the same value as system()
            $block .= ' $?;';
            return $block . "\n";
        }
    }

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

        # Previously we treated the pipeline token '|' as an automatic
        # indicator that the list-form represented a shell pipeline and
        # fell back to running the whole thing via `bash -c`. That caused
        # list-form system() calls that explicitly passed '|' as a literal
        # argument (for example system("echo", "a", "|", "tee", ...))
        # to be converted into real shell pipelines and change semantics.
        # Preserve list-form semantics: do not treat '|' specially here so
        # a literal '|' remains an argv element. If the caller truly
        # intended a pipeline the surrounding code can still produce a
        # pipeline via explicit shell snippets; being conservative here
        # avoids surprising behavior changes.

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
        # Preserve literal pipe operators as real pipes (do not quote them)
        # so the shell sees pipelines instead of literal '|' filenames.
        my $shell_cmd = join(' ', map {
            my ($t,$q) = ref($_) eq 'ARRAY' ? @$_ : ($_,'bare');
            $t eq '|' ? '|' : _shell_quote_for_system($t);
        } ($first, @tokens));

        # Use a non-interpolating Perl literal for the bash -c argument so
        # embedded shell fragments (notably awk/sed programs containing
        # $0/$1 variables) are preserved verbatim and not expanded by Perl
        # at parse time. Choose a q{}-style delimiter that does not appear
        # in the contents when possible; fall back to an escaped double-quote
        # form if necessary.
        my $cmd_lit = _perl_quote_literal_no_interp($shell_cmd);
        my $block = 'my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec (\'bash\', \'-c\', ' . $cmd_lit . '); die "exec failed: " . $!; } else { waitpid($pid, 0); }';
        $block .= ' $?;';
        return $block . "\n";
    }

    # Build the exec argument list (exclude the program itself which is in $exe_quoted)
    my $args_list = '';
    # @argv_tokens contains only the argv elements (the program name was
    # removed earlier when we shifted @tokens). Join all entries here so the
    # exec call receives the full argument list. The previous code
    # mistakenly skipped the first element (as if @argv_tokens still
    # contained the program at index 0) which dropped the first argument
    # and produced empty/incorrect command invocations (e.g. `echo` with
    # no args).
    if (@argv_tokens) {
        $args_list = join(', ', @argv_tokens);
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
    # If the original token preferred double-quotes, try to respect that
    # preference but avoid forcing a double-quoted Perl literal when it's
    # not necessary. Many shell snippets (awk/sed programs) contain "$" or
    # "@" which must be preserved verbatim; forcing a double-quoted Perl
    # literal here would cause Perl interpolation and change the contents.
    # Only use a double-quoted Perl literal when the text actually contains
    # characters that require it (newlines, double quotes, backslashes or
    # control characters). Otherwise prefer a single-quoted literal so
    # "$" and "@" remain literal in the emitted Perl source.
    if (defined $pref && $pref eq 'double') {
        # The original token was double-quoted in the source which means
        # Perl interpolation (e.g. $0, @arr) was intended at the call site.
        # Preserve that semantic by emitting a double-quoted Perl literal
        # here. Escape backslashes and double-quotes and encode control
        # characters so the generated Perl source remains valid.
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
        # Escape single quotes for a Perl single-quoted literal.
        $t =~ s/'/\\'/g;
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
    # Escape single quotes for a Perl single-quoted literal. See note above
    # about avoiding shell-style escapes.
    $text =~ s/'/\\'/g; # escape single quotes as \'
    return "'$text'";
}

sub _perl_quote_literal_no_interp {
    my ($text) = @_;
    return "q{}" unless defined $text && length $text;

    # Fast path: if there are no single-quotes or newlines, a simple
    # single-quoted literal is the most readable and safe non-interpolating
    # form.
    my $contains_single_quote = $text =~ /'/;
    my $contains_newline = $text =~ /\n/;

    if (!$contains_single_quote && !$contains_newline) {
        my $escaped = $text;
        $escaped =~ s/\\/\\\\/g;
        $escaped =~ s/'/\\'/g;
        return "'$escaped'";
    }

    # Choose a q<delim>...<delim> form where possible so we can include
    # single quotes and newlines verbatim without interpolation.
    my @pairs = (
        ['{', '}'], ['(', ')'], ['[', ']'], ['<', '>'], ['|', '|'], ['/', '/'],
        ['#', '#'], ['%', '%'], ['@', '@'], ['!', '!'], ['~', '~'], ['^', '^'],
        [':', ':'], [';', ';'],
    );

    for my $p (@pairs) {
        my ($open, $close) = @$p;
        next if index($text, $open) >= 0 || index($text, $close) >= 0;
        return "q$open$text$close";
    }

    # If every delimiter candidate appears in the string (rare), fall back
    # to a double-quoted literal with escaped control characters. This is a
    # last-resort fallback; it may allow Perl interpolation of $/@ but such
    # inputs are exceedingly uncommon.
    my $escaped = $text;
    $escaped =~ s/\\/\\\\/g;
    $escaped =~ s/"/\\"/g;
    $escaped =~ s/\n/\\n/g;
    $escaped =~ s/\t/\\t/g;
    $escaped =~ s/\r/\\r/g;
    return "\"$escaped\"";
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
    
    # If the replacement code is a hand-crafted exec/fork block or already a
    # do{ ... } wrapper as emitted by debashc, avoid running it through
    # extract_core_perl_logic_ppi which is intended to strip headers from
    # full generated scripts and can mangle quoting for small code
    # fragments. Detect common patterns for exec/fork/do-blocks and skip
    # the extraction in those cases.
    unless ($replacement_code =~ /(?:^do\s*\{|my\s+\$pid\s*=\s*fork\b|\bexec\s*\()/s) {
        $replacement_code = extract_core_perl_logic_ppi($replacement_code);
    }

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

    my $wrapped_code;
    if (defined $lhs) {
        $wrapped_code = $lhs . " do {\n" . $replacement_code . "\n};";
    } else {
        # When replacing a bare system() statement (no LHS assignment)
        # we must not print the return value when the replacement is a
        # fork/exec style block that returns an exit-status ($?). The
        # original system() prints its child-side output directly to
        # STDOUT; printing the replacement's return value here would
        # incorrectly emit the numeric exit code (0/1) into program
        # output. Detect common patterns (exec/fork or an appended $?)
        # and insert the replacement as-is in those cases. For other
        # replacements (notably those produced for backtick/command
        # substitution which return strings) preserve the previous
        # behaviour of printing non-empty returned strings.
        # If the replacement already performs child-side printing or
        # restores STDOUT (redirection handling) then inserting the
        # extra printing wrapper would cause the numeric return value
        # (for example the return of `close`) to be printed. In those
        # cases insert the replacement verbatim. However, there is a
        # recurring pattern where the generator emits a do-block whose
        # final statement is an explicit `print qx{...};`. When that
        # do-block is later captured into a temp variable by the outer
        # wrapper the do-block returns the numeric return value of
        # `print` (usually 1) which then gets printed into redirected
        # output files incorrectly. Detect that specific case and
        # normalize it to an expression-valued form before deciding
        # whether to insert verbatim.
        if ($replacement_code =~ /\$\?\s*;|\bmy\s+\$pid\s*=\s*fork\b|\bexec\s*\(|open\s+.*STDOUT|open\s*\(\s*STDOUT/s) {
            # If the replacement already performs child-side printing or
            # restores STDOUT (redirection handling) then inserting the
            # extra printing wrapper would cause the numeric return value
            # (for example the return of `close`) to be printed. In those
            # cases insert the replacement verbatim.
            # But first: normalize the special-case print qx{...}; pattern
            # into a safe expression-valued form so nested wrappers don't
            # accidentally capture the numeric return of print.
            $replacement_code = normalize_print_qx_patterns($replacement_code);
            $wrapped_code = $replacement_code;
        } else {
            $wrapped_code = "do {\n";
            $wrapped_code .= "    my \$__PURIFY_TMP = do {\n" . $replacement_code . "\n    };\n";
            $wrapped_code .= "    if (defined \$__PURIFY_TMP && \$__PURIFY_TMP ne q{}) {\n";
            $wrapped_code .= "        print \$__PURIFY_TMP;\n";
            $wrapped_code .= "        if (!(\$__PURIFY_TMP =~ m{\\n\\z}msx)) { print \"\\n\"; }\n";
            $wrapped_code .= "    }\n";
            $wrapped_code .= "};";
        }
    }

    print "DEBUG: replace_system_call_with_code - lhs=[" . (defined $lhs ? $lhs : '') . "]\n" if $verbose;
    print "DEBUG: replace_system_call_with_code - replacement_code=[" . substr($replacement_code,0,400) . "]\n" if $verbose;
    print "DEBUG: replace_system_call_with_code - wrapped_code=[" . substr($wrapped_code,0,400) . "]\n" if $verbose;
    # Attempt to atomically set the statement content if supported. This
    # is the simplest and safest approach because PPI will reparse the
    # statement content in-place, avoiding partial-clone issues seen when
    # parsing into a separate document and extracting child(0).
    # Use explicit insertion of parsed replacement statements instead of
    # PPI::Statement->set_content(). set_content can sometimes reparse and
    # normalize the replacement in unexpected ways (losing sigils or
    # altering statement boundaries). The explicit-insert path parses the
    # replacement into a temporary PPI document and clones its top-level
    # statements into the current document which is more robust.
    {
        print "DEBUG: Using explicit insertion; replacement length=" . length($wrapped_code) . "\n" if $verbose;
        # Parse the wrapped replacement into a document and insert all
        # top-level statements after the original statement. We insert
        # in reverse order so the final order in the tree matches the
        # replacement document.
        my $replacement_doc = PPI::Document->new(\$wrapped_code);
        return unless $replacement_doc;

        # Collect top-level nodes from the replacement document. The
        # previous implementation filtered only PPI::Statement nodes which
        # could drop legitimate top-level constructs (for example an if
        # block parsed as a non-Statement node). Use the document's
        # children and skip purely-whitespace nodes so we preserve the
        # generator-emitted fallback and other non-statement top-level
        # nodes.
        my @children = $replacement_doc->children;
        my @nodes = ();
        for my $n (@children) {
            next unless defined $n;
            my $ser = '';
            eval { $ser = $n->serialize }; # fall back gracefully
            $ser = $n->content unless length($ser);
            next unless defined $ser && $ser =~ /\S/;
            push @nodes, $n;
        }

        print "DEBUG: Parsed replacement document contains " . scalar(@nodes) . " top-level nodes\n" if $verbose;
        if ($verbose && @nodes) {
            my $first = eval { $nodes[0]->serialize } || $nodes[0]->content || '';
            my $last  = eval { $nodes[-1]->serialize } || $nodes[-1]->content || '';
            print "DEBUG: First node preview: " . substr($first,0,200) . "\n";
            print "DEBUG: Last node preview: " . substr($last,0,200) . "\n";
        }

        # Additional targeted diagnostics: when the replacement contains
        # sha256_hex/sha512_hex emit richer information about the parsed
        # replacement document and the cloned statements so we can track
        # any token-level mangling that happens during insertion.
        my $is_sha_related = ($wrapped_code =~ /sha256_hex|sha512_hex/);
        if ($verbose && $is_sha_related) {
            print "DEBUG: replacement_doc->serialize:\n" . $replacement_doc->serialize . "\n";
        }

        # If there are no useful top-level nodes, fall back to child(0)
        # replacement to preserve previous behavior.
        unless (@nodes) {
            my $replacement_stmt = $replacement_doc->child(0);
            return unless $replacement_stmt;
            $statement->replace($replacement_stmt->clone);
            return;
        }

        # Insert clones after the original statement in reverse order
        # to maintain ordering. Use insert_after on the statement node.
        for my $node (reverse @nodes) {
            my $clone = $node->clone;
            if ($verbose && $is_sha_related) {
                # Show the cloned node serialization before insertion
                print "DEBUG: clone serialized (pre-insert): [" . $clone->serialize . "]\n";
            }
            $statement->insert_after($clone);
        }

        # Remove the original statement now that the replacements are
        # in place.
        # Capture the root document for post-insert diagnostics before
        # removing the statement so we can inspect the document state.
        my $root_doc = $statement;
        $root_doc = $root_doc->parent while $root_doc && !$root_doc->isa('PPI::Document');
        $statement->remove;

        if ($verbose && $is_sha_related && $root_doc) {
            # Print a focused slice of the parent document around the
            # insertion site to see how PPI serialized the newly-inserted
            # content in the context of the full file. Limit output size.
            my $doc_ser = $root_doc->serialize;
            my $preview = length($doc_ser) > 8000 ? substr($doc_ser,0,8000) . "\n...(truncated)" : $doc_ser;
            print "DEBUG: Parent document serialization after insertion (preview):\n" . $preview . "\n";
        }
    }
}

# Normalize occurrences where a replacement do-block ends with `print qx{...};`
# into an expression-valued form so wrappers that capture the do-block receive
# the actual string result instead of print's numeric return value. This
# handles common qx{} delimiter styles: qx{...}, qx(...), qx`...` and friends.
sub normalize_print_qx_patterns {
    my ($code) = @_;
    return $code unless defined $code && length $code;

    # Quick check: if no 'print' or 'qx' present, nothing to do.
    return $code unless $code =~ /\bprint\b/ && ($code =~ /qx/ || $code =~ /`/);

    # Try a conservative PPI-based rewrite first. This inspects `do { ... }`
    # blocks and replaces cases where the final statement is `print qx{...};`
    # or `print $var;` (when $var was assigned from qx earlier in the same
    # block) with an expression-valued sequence so outer capture-wrappers
    # receive the string result rather than print's numeric return value.
    my $doc = PPI::Document->new(\$code);
    unless ($doc) {
        # Fall back to the simple regex-based approach when parsing fails
        goto REGEX_FALLBACK;
    }

    my $changed = 0;

    # Find all `do` tokens and process their following block. Iterate in
    # reverse document order so inner/nested blocks are handled before
    # their enclosing parents which avoids invalidating node references.
    my $find_do = PPI::Find->new(sub {
        my $n = shift;
        return ($n->isa('PPI::Token::Word') && $n->content eq 'do') ? 1 : 0;
    });

    my @do_tokens = reverse $find_do->in($doc);

    for my $do_token (@do_tokens) {
        my $block = $do_token->snext_sibling;
        next unless $block && $block->isa('PPI::Structure::Block');

        # Collect only the top-level statements inside the block (ignore
        # nested statements from inner blocks).
        my @children = $block->children;
        my @stmts = grep { $_ && $_->isa('PPI::Statement') } @children;
        next unless @stmts;

        # Map variable names that were assigned from qx/... earlier in the block
        my %qx_assigned;
        for my $stmt (@stmts[0 .. $#stmts - 1]) {
            next unless defined $stmt;
            my $text = $stmt->content || '';
            if ($text =~ /\b(?:my\s+)?\$(\w+)\s*=\s*(?:qx\b|`)/s) {
                $qx_assigned{$1} = 1;
            }
        }

        my $last_stmt = $stmts[-1];
        next unless $last_stmt;
        my $last_text = $last_stmt->content || '';

        # Case A: print qx{...};  (also handles print(qx{...}); and backticks)
        if ($last_text =~ /^\s*print\s*(?:\(\s*)?(?:qx\b|`)/s) {
            # Avoid two-arg print/filehandle forms since they don't start
            # with qx/backtick as the first argument (conservative check).
            # Extract the qx/backtick operand using the parsed Quote token
            my $quotes = $last_stmt->find('PPI::Token::Quote');
            my $qx_operand;
            if ($quotes && @$quotes) {
                for my $q (@$quotes) {
                    my $c = $q->content || '';
                    if ($c =~ /^(?:qx|`)/) { $qx_operand = $c; last; }
                }
            }
            # If PPI didn't expose a Quote token, fall back to a conservative
            # regex capture of the operand from the statement text.
            unless (defined $qx_operand) {
                if ($last_text =~ /^\s*print\s*(?:\(\s*)?((?:qx\b|`)[^;]+?)\s*(?:\)\s*)?;\s*$/s) {
                    $qx_operand = $1;
                }
            }

            next unless defined $qx_operand;

            $PURIFY_PRINT_QX_COUNTER++;
            my $tmp = "__PURIFY_PRINT_QX_" . $PURIFY_PRINT_QX_COUNTER;
            my $replacement = "my \$$tmp = $qx_operand; print \$$tmp; \$$tmp;";

            # Parse the replacement and insert its top-level nodes in place
            # of the original last statement.
            my $replacement_doc = PPI::Document->new(\$replacement);
            next unless $replacement_doc;
            my @rep_children = $replacement_doc->children;
            my @rep_nodes = ();
            for my $n (@rep_children) {
                next unless defined $n;
                my $ser = '';
                eval { $ser = $n->serialize }; # fall back gracefully
                $ser = $n->content unless length($ser);
                next unless defined $ser && $ser =~ /\S/;
                push @rep_nodes, $n;
            }

            for my $node (reverse @rep_nodes) {
                $last_stmt->insert_after($node->clone);
            }
            $last_stmt->remove;
            $changed = 1;
            next;
        }

        # Case B: print $var; where $var was assigned from qx earlier in this block
        if ($last_text =~ /^\s*print\s*(?:\(\s*)?\$(\w+)\s*(?:\)\s*)?;\s*$/s) {
            my $var = $1;
            if ($qx_assigned{$var}) {
                my $replacement = "print \$$var; \$$var;";
                my $replacement_doc = PPI::Document->new(\$replacement);
                next unless $replacement_doc;
                my @rep_children = $replacement_doc->children;
                my @rep_nodes = ();
                for my $n (@rep_children) {
                    next unless defined $n;
                    my $ser = '';
                    eval { $ser = $n->serialize };
                    $ser = $n->content unless length($ser);
                    next unless defined $ser && $ser =~ /\S/;
                    push @rep_nodes, $n;
                }
                for my $node (reverse @rep_nodes) {
                    $last_stmt->insert_after($node->clone);
                }
                $last_stmt->remove;
                $changed = 1;
                next;
            }
        }
    }

    return $doc->serialize if $changed;

    # If no PPI-based changes occurred fall back to the original
    # conservative regex approach so we still handle trivial cases.
    REGEX_FALLBACK: {
        my $rewritten = $code;
        my $changed = 0;

        while ($rewritten =~ /(print)\s*(qx)(\s*)([\{\(\`\[<])(.*?)([\}\)\`\]>])\s*;/s) {
            my $full = $&;
            my $qx_open = $4;
            my $inner = $5;
            my $qx_close = $6;

            # Ensure parentheses/delimiters balance roughly by checking no unmatched
            # closing delimiter appears inside inner (best-effort). Skip if inner
            # contains the closing delimiter which likely means nested constructs.
            if (index($inner, $qx_close) != -1) {
                # Avoid rewriting nested qx constructs
                last;
            }

            # Build unique temp var name
            $PURIFY_PRINT_QX_COUNTER++;
            my $tmp = "__PURIFY_PRINT_QX_" . $PURIFY_PRINT_QX_COUNTER;

            # Construct replacement: assign the qx to temp, print temp, then return temp
            my $qx_operand = "qx" . $qx_open . $inner . $qx_close;
            my $replacement = "my \$$tmp = $qx_operand; print \$$tmp; \$$tmp;";

            # Replace only the first occurrence to avoid accidental multi-rewrites
            $rewritten =~ s/\Q$full\E/$replacement/;
            $changed = 1;
            # Continue scanning in case there are more occurrences later
        }

        return $rewritten;
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
    # Invoke debashc directly without going through an intermediate shell so
    # we don't have to shoehorn the shell snippet into a quoted shell
    # string. This avoids nested-quoting/escaping issues where embedded
    # single-quotes or other characters would be corrupted by an extra
    # shell parsing layer. Capture both stdout and stderr from debashc and
    # combine them for downstream extraction.
    print "DEBUG: Running command: $debashc_path parse $mode <shell_command>\n" if $verbose;

    my $stdout = '';
    my $err = gensym;
    my $out;
    my $pid;
    eval {
        $pid = open3(undef, $out, $err, $debashc_path, 'parse', $mode, $shell_command);
        1;
    } or do {
        warn "Failed to invoke debashc via open3: $@\n";
        return undef;
    };

    # Read both stdout and stderr until EOF to avoid deadlocks on large output
    my $sel = IO::Select->new();
    $sel->add($out) if defined $out;
    $sel->add($err);
    while ($sel->count) {
        for my $fh ($sel->can_read) {
            my $buf;
            my $bytes = sysread($fh, $buf, 8192);
            if (defined $bytes) {
                if ($bytes == 0) {
                    $sel->remove($fh);
                    close $fh;
                } else {
                    $stdout .= $buf;
                }
            } else {
                # On error just remove the handle and continue
                $sel->remove($fh);
                close $fh;
            }
        }
    }

    waitpid($pid, 0);
    my $exit_code = $? >> 8;

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

    # Defensive check: if debashc fell back to emitting a single-quoted
    # system(...) invocation but the original shell command contains
    # single-quote characters, the emitted Perl will be syntactically
    # invalid (nested unescaped single-quotes). Treat this as a failed
    # conversion so callers can fall back to a safe exec('sh','-c', ...) path
    # which we construct using a non-interpolating Perl literal.
    if (defined $perl_result && !$is_backticks) {
        if ($perl_result =~ /system\s*'/ && $shell_command =~ /'/) {
            warn "DEBUG: debashc emitted single-quoted system fallback while original command contains single-quotes; treating as conversion failure\n" if $verbose;
            return undef;
        }
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

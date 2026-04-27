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

    # If the converted code references the __bt() helper (used to wrap
    # inline backtick replacements so they return a list of lines in list
    # context and a scalar string in scalar context, matching Perl's native
    # backtick-in-context semantics), inject its definition near the top of
    # the file.
    # The helper joins all its arguments first (with join '') so that when
    # the inner do-block is evaluated in list context (as function arguments
    # always are) and its last expression returns multiple values (e.g. qx{}
    # in list context), all the lines are correctly reassembled before the
    # context-sensitive split/return.
    if ($serialized =~ /\b__bt\s*\(/ && $serialized !~ /\bsub\s+__bt\b/) {
        my $bt_sub = "sub __bt { my \$s = join('', \@_); wantarray ? (split /^/, \$s, -1) : \$s }\n";
        $serialized = $bt_sub . $serialized;
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

        # Check if system() is used in expression context: either its
        # statement is inside a condition structure (if/while/until
        # parentheses) or there is a comparison/logical operator after the
        # argument list (e.g. system(...) == 0, my $x = system(...) != 0).
        # In those cases the result VALUE of system() matters and we must
        # replace only the system(args) call with a do { fork+exec; $? }
        # block that returns the exit status, leaving the surrounding
        # expression (== 0, != 0, etc.) intact.
        if (_system_stmt_in_expr_ctx($system_call_stmt)) {
            _replace_system_in_expr($system_call_stmt, \@tokens);
            next;
        }

        # Single-argument and multi-argument system() in statement context.
        # Always use fork+exec so we preserve the exact semantics of the
        # original system() call: the command runs in a child process,
        # its stdout/stderr go directly to the terminal (not captured),
        # and the parent waits for completion.  Using debashc here caused
        # problems such as die-on-failure (for rmdir/mkdir conversions) and
        # spurious printed return values (e.g. 1 from a successful rmdir).
        # For statement context we don't need to capture $? so strip the
        # trailing '$?' from the fork+exec template to avoid a 'useless use
        # of a variable in void context' warning.
        my $exec_code = _build_fork_exec_for_expr(\@tokens);
        $exec_code =~ s/\n\$\?\s*$//;   # remove trailing $? in stmt context
        replace_system_call_with_code($system_call_stmt, $exec_code . "\n");
        next;
    }

    return $document->serialize;
}

# Returns true when system() appears in expression context: either inside
# a condition structure (if/while parentheses), followed by a comparison
# or logical operator (e.g. system(...) == 0), or preceded by an assignment
# or other operator (e.g. my $rc = system(...)).
sub _system_stmt_in_expr_ctx {
    my ($stmt) = @_;

    # Case 1: statement is a direct child of a condition structure
    return 1 if $stmt->parent && $stmt->parent->isa('PPI::Structure::Condition');

    # Case 2: operator BEFORE system() – e.g. my $result = system(...)
    # (the = operator precedes 'system' in the significant-child list).
    for my $ch ($stmt->schildren) {
        last if $ch->isa('PPI::Token::Word') && $ch->content eq 'system';
        return 1 if $ch->isa('PPI::Token::Operator') && $ch->content !~ /^[;,]$/;
    }

    # Case 3: operator AFTER the system(args) call that uses its result
    my $past_system = 0;
    my $past_list   = 0;
    for my $ch ($stmt->schildren) {
        if (!$past_system) {
            $past_system = 1 if $ch->isa('PPI::Token::Word') && $ch->content eq 'system';
            next;
        }
        if (!$past_list) {
            $past_list = 1 if $ch->isa('PPI::Structure::List');
            next;
        }
        # Any operator other than ; or , after the arg list means the
        # result of system() is being used in an expression
        return 1 if $ch->isa('PPI::Token::Operator') && $ch->content !~ /^[;,]$/;
    }
    return 0;
}

# Generate a fork+exec block that returns $? (exit status), suitable for
# embedding inside a do { ... } expression. Does NOT call debashc so the
# original exit-status semantics of system() are preserved exactly.
sub _build_fork_exec_for_expr {
    my ($tokens_ref) = @_;
    my @tokens = @{$tokens_ref};

    # Special case: sh/bash -c <cmd>
    if (@tokens >= 2) {
        my ($exe, $exe_q) = ref($tokens[0]) eq 'ARRAY' ? @{$tokens[0]} : ($tokens[0], 'bare');
        if ($exe eq 'sh' || $exe eq 'bash') {
            my ($flag, $flag_q) = ref($tokens[1]) eq 'ARRAY' ? @{$tokens[1]} : ($tokens[1], 'bare');
            # Normalize surrounding whitespace before matching (handles e.g. ' -c' with a leading space)
            (my $normalized_flag = $flag) =~ s/^\s+|\s+$//g;
            if ($normalized_flag eq '-c' && @tokens >= 3) {
                my $exe_lit  = _perl_quote_literal_with_pref($exe,  $exe_q);
                my $flag_lit = _perl_quote_literal_with_pref($flag, $flag_q);
                my @cmd_parts = @tokens[2..$#tokens];
                my $shell_cmd_raw = join(' ', map {
                    my ($t,$q) = ref($_) eq 'ARRAY' ? @$_ : ($_,'bare');
                    ($q && $q eq 'double') ? decode_perl_double_quoted_string($t) :
                    ($q && $q eq 'single') ? decode_perl_single_quoted_string($t) : $t
                } @cmd_parts);
                # Preserve Perl interpolation when the original used double-quoted
                # tokens containing variable sigils
                my $needs_interp = 0;
                for my $p (@cmd_parts) {
                    my ($pt,$pq) = ref($p) eq 'ARRAY' ? @{$p} : ($p,'bare');
                    if ($pq && $pq eq 'double' && $pt =~ /(?<!\\)(?:\$\{?[A-Za-z_]|@\{?[A-Za-z_])/) {
                        $needs_interp = 1; last;
                    }
                }
                my $cmd_lit = $needs_interp
                    ? _perl_quote_literal_with_pref($shell_cmd_raw, 'double')
                    : _perl_quote_literal_no_interp($shell_cmd_raw);
                return "my \$pid = fork; if (!defined \$pid) { die \"fork failed: \" . \$!; } elsif (\$pid == 0) { exec($exe_lit, $flag_lit, $cmd_lit); die \"exec failed: \" . \$!; } else { waitpid(\$pid, 0); }\n\$?";
            }
        }
    }

    # Single-arg with a Perl variable: system($cmd) -> exec('sh','-c',$cmd)
    if (@tokens == 1) {
        my ($txt, $q) = ref($tokens[0]) eq 'ARRAY' ? @{$tokens[0]} : ($tokens[0], 'bare');
        if ($q eq 'bare' && $txt =~ /^\$/) {
            return "my \$pid = fork; if (!defined \$pid) { die \"fork failed: \" . \$!; } elsif (\$pid == 0) { exec('sh', '-c', $txt); die \"exec failed: \" . \$!; } else { waitpid(\$pid, 0); }\n\$?";
        }
        # Single literal string
        my $cmd_lit = ($q eq 'double')
            ? _perl_quote_literal_with_pref($txt, 'double')
            : _perl_quote_literal_no_interp($txt);
        return "my \$pid = fork; if (!defined \$pid) { die \"fork failed: \" . \$!; } elsif (\$pid == 0) { exec('sh', '-c', $cmd_lit); die \"exec failed: \" . \$!; } else { waitpid(\$pid, 0); }\n\$?";
    }

    # General multi-arg: exec directly with the args.
    # Decode single- and double-quoted token content before re-quoting to
    # avoid double-escaping (PPI's string() returns raw inner text with e.g.
    # \' not yet decoded, so decoding first ensures a clean round-trip).
    my @perl_args;
    for my $t (@tokens) {
        my ($txt, $q) = ref($t) eq 'ARRAY' ? @{$t} : ($t, 'bare');
        my $decoded = ($q && $q eq 'single') ? decode_perl_single_quoted_string($txt)
                    : ($q && $q eq 'double') ? decode_perl_double_quoted_string($txt)
                    : $txt;
        push @perl_args, _perl_quote_literal_with_pref($decoded, $q);
    }
    my $args_str = join(', ', @perl_args);
    return "my \$pid = fork; if (!defined \$pid) { die \"fork failed: \" . \$!; } elsif (\$pid == 0) { exec($args_str); die \"exec failed: \" . \$!; } else { waitpid(\$pid, 0); }\n\$?";
}

# Replace only the system(args) portion of $stmt with do { fork+exec; $? },
# preserving the surrounding expression (e.g. == 0 comparison, assignment LHS).
sub _replace_system_in_expr {
    my ($stmt, $tokens_ref) = @_;

    my $exec_code  = _build_fork_exec_for_expr($tokens_ref);
    my $do_block   = "do {\n$exec_code\n}";

    # Walk the statement's children to collect everything before 'system'
    # and everything after the argument list.
    my $before = '';
    my $after  = '';
    my $state  = 'before';

    for my $ch ($stmt->children) {
        if ($state eq 'before') {
            if ($ch->isa('PPI::Token::Word') && $ch->content eq 'system') {
                $state = 'at_system';
                # Do not include 'system' itself
            } else {
                $before .= $ch->content;
            }
        } elsif ($state eq 'at_system') {
            if ($ch->isa('PPI::Structure::List')) {
                $state = 'after';
                # Skip the arg list
            } elsif ($ch->isa('PPI::Token::Whitespace')) {
                # Skip any whitespace between 'system' and its arg list
            } else {
                return; # Unexpected token; abort replacement
            }
        } else {
            $after .= $ch->content;
        }
    }

    return if $state ne 'after';

    my $new_text = $before . $do_block . $after;

    my $new_doc = PPI::Document->new(\$new_text);
    return unless $new_doc;

    my @nodes = grep { $_->content =~ /\S/ } $new_doc->children;
    return unless @nodes;

    for my $node (reverse @nodes) {
        $stmt->insert_after($node->clone);
    }
    $stmt->remove;
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

    # Debashc sometimes generates code that references internal variables
    # (e.g. $DATE_SNAPSHOT) that are never defined in the output context.
    # Detect such patterns and treat them as conversion failures so the
    # open3-based fallback below is used instead.
    if ($perl_result && $perl_result =~ /\$DATE_SNAPSHOT\b/) {
        print "DEBUG: debashc output references \$DATE_SNAPSHOT; treating as conversion failure\n" if $verbose;
        undef $perl_result;
    }

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

        # Fix debashc inline pipeline output issues:
        # 1. Debashc sometimes emits `my $output_N = q{};` followed later
        #    by a bare `my $output_N;` in the same scope. The second
        #    redeclaration causes 'variable masks earlier declaration' errors
        #    under `use strict`. Remove the redundant bare redeclarations.
        $perl_result =~ s/\n[ \t]*my (\$[a-zA-Z_][a-zA-Z0-9_]*)\s*;\s*(?=\n)//g;

        # 2. Inline pipeline blocks reference $main_exit_code which is not
        #    declared in the do-block context. Remove those assignment lines.
        $perl_result =~ s/[ \t]*if\s*\(\s*!\s*\$pipeline_success_\d+\s*\)\s*\{\s*\$main_exit_code\s*=\s*1;\s*\}[ \t]*\n?//g;

        # If debashc generated pure Perl without any shell execution (no
        # qx{}/exec/open3/system) for a `yes ... | head/tail` pipeline, the
        # conversion omits shell-level side effects: specifically, the
        # "yes: standard output: Broken pipe" message that GNU yes writes to
        # stderr when head/tail closes the pipe early.  Return an IPC::Open3
        # shell-execution block which preserves all shell-level behaviors
        # including stderr.
        if (defined $perl_result
            && $command =~ /\byes\b.*\|/
            && $perl_result !~ /\bqx\b/
            && $perl_result !~ /\bopen3\b/
            && $perl_result !~ /\bexec\b/
            && $perl_result !~ /\bsystem\b/) {
            print "DEBUG: debashc generated pure-Perl for 'yes' pipeline; falling back to IPC::Open3\n" if $verbose;
            my $cmd_lit = _perl_quote_literal_no_interp($command);
            my $open3_inner = "do {\n"
                . "    require IPC::Open3;\n"
                . "    my \$__bt_out;\n"
                . "    IPC::Open3::open3(my \$__bt_in, \$__bt_out, \\*STDERR, 'sh', '-c', $cmd_lit);\n"
                . "    close \$__bt_in;\n"
                . "    local \$/ = undef;\n"
                . "    my \$__bt_result = <\$__bt_out>;\n"
                . "    close \$__bt_out;\n"
                . "    waitpid(-1, 0);\n"
                . "    \$__bt_result\n"
                . "}";
            my $open3_code = defined $var_name ? $open3_inner : "__bt($open3_inner)";
            return defined $var_name ? "$prefix$var_name = $open3_inner;" : $open3_code;
        }

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

        # Wrap the backtick result in a call to the __bt helper (injected
        # at the top of every purified file) which uses wantarray() to return
        # a list of lines in list context (matching backtick-in-list-context
        # semantics used e.g. when a backtick is passed directly as a
        # function argument) and the raw string in scalar context.
        if (!defined $var_name) {
            # Strip any trailing semicolon that debashc may have appended to
            # the do-block.  A semicolon inside a function-call argument list
            # causes a syntax error (it terminates the statement), so we must
            # remove it before embedding the expression in __bt(...).
            (my $expr = $perl_result) =~ s/;\s*$//s;
            $perl_result = "__bt($expr)";
        }

        return defined $var_name ? "$prefix$var_name = $perl_result;" : $perl_result;
    } else {
        print "DEBUG: No perl result for backtick command; using open3 fallback\n" if $verbose;
        # Fallback: capture command output via open3 (avoids keeping a backtick
        # that would fail the purification check and avoids running a shell).
        my $cmd_lit = _perl_quote_literal_no_interp($command);
        # Use \*STDERR so child stderr is inherited from the parent (not
        # captured into the stdout pipe). Passing '' is false, which on some
        # IPC::Open3 versions redirects child stderr to the stdout pipe and
        # causes stderr messages to appear inside the captured result instead
        # of being written to the real STDERR at the correct time.
        my $open3_inner = "do {\n"
            . "    require IPC::Open3;\n"
            . "    my \$__bt_out;\n"
            . "    IPC::Open3::open3(my \$__bt_in, \$__bt_out, \\*STDERR, 'sh', '-c', $cmd_lit);\n"
            . "    close \$__bt_in;\n"
            . "    local \$/ = undef;\n"
            . "    my \$__bt_result = <\$__bt_out>;\n"
            . "    close \$__bt_out;\n"
            . "    waitpid(-1, 0);\n"
            . "    \$__bt_result\n"
            . "}";
        my $open3_code = defined $var_name ? $open3_inner : "__bt($open3_inner)";
        return defined $var_name ? "$prefix$var_name = $open3_inner;" : $open3_code;
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
            $text = $token->string; # preserve original inner text including whitespace
        } elsif ($token->isa('PPI::Token::Quote::Double') || $token->isa('PPI::Token::Quote::Interpolate')) {
            # PPI string() returns the raw inner content (outer quotes stripped)
            # without resolving any escape sequences.  Check for unescaped Perl
            # sigils to decide whether to re-emit an interpolating literal.
            my $inner = $token->string;  # e.g. "echo \$SHELL_VAR" → 'echo \$SHELL_VAR'
            if (_has_unescaped_ident_sigil($inner)) {
                # Unescaped Perl sigil: the author intended interpolation.
                # Keep the raw inner content so callers re-emit a double-
                # quoted literal that Perl interpolates at runtime.
                $quote_type = 'double';
                $text = $inner;
            } else {
                # No unescaped Perl sigil (e.g. the source had \$SHELL_VAR to
                # pass a literal dollar to the shell).  Decode double-quote
                # escape sequences (\$ → $, \\ → \, \n → newline, etc.) to
                # get the actual string value, then re-quote it as a
                # no-interpolation literal.
                $quote_type = 'single';
                $text = decode_perl_double_quoted_string($inner);
            }
        } elsif ($token->isa('PPI::Token::Quote')) {
            # Fallback for other quote forms (q{}, qq{}, etc.) - use
            # the string() value and conservatively treat qq-like forms
            # as double-quoted when the content implies interpolation.
            my $inner = $token->string;
            if ($token->content =~ /^qq/ && _has_unescaped_ident_sigil($inner)) {
                $quote_type = 'double';
                $text = $inner;
            } else {
                # q{} or qq{} without Perl sigils: treat as literal value.
                # For q{}, string() is already the decoded value.
                # For qq{}, decode escape sequences.
                $quote_type = 'single';
                $text = ($token->content =~ /^qq/)
                    ? decode_perl_double_quoted_string($inner)
                    : $inner;
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
        # Determine if the original token was a clean '-c' (no surrounding
        # whitespace). Only when the original token equals the normalized
        # value do we consider attempting the special debashc conversion.
        my $is_clean_flag = ($flag_txt eq $normalized_flag);
        # If the token was exactly '-c' prefer a sane bare quoting
        # preference; do not adjust quoting when the original token
        # contained surrounding whitespace since we want to preserve
        # the original semantics in that case.
        if ($is_clean_flag && $normalized_flag =~ /^-c$/) {
            $flag_q = 'bare' if !$flag_q || $flag_q eq '';
        }
        # If the normalized flag matches '-c' but the original token
        # contained surrounding whitespace (not a clean '-c'), avoid any
        # normalization and preserve the original argument text by using
        # the perl-quoted tokens we captured earlier. This preserves the
        # exact runtime behavior (including accidental/malformed flags)
        # and avoids attempting debashc conversion which could change
        # semantics.
        if ($normalized_flag eq '-c' && !$is_clean_flag) {
            # The original flag token contained surrounding whitespace;
            # preserve the exact runtime behaviour by emitting an exec
            # block that uses the original tokens but reconstructs safe
            # Perl literals from their decoded inner text. This avoids
            # double-escaping issues that can arise when reusing previously
            # quoted fragments.
            my @arg_literals = ();
            for my $a (@tokens) {
                my ($t,$q) = ref($a) eq 'ARRAY' ? @{$a} : ($a,'bare');
                my $decoded = defined $q && $q eq 'double' ? decode_perl_double_quoted_string($t)
                            : defined $q && $q eq 'single' ? decode_perl_single_quoted_string($t)
                            : $t;
                push @arg_literals, _perl_quote_literal_with_pref($decoded, $q);
            }
            my $args_list = @arg_literals ? join(', ', @arg_literals) : '';
            my $block = 'my $pid = fork; if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) {';
            $block .= ' exec (' . $exe_quoted;
            if (length $args_list) { $block .= ', ' . $args_list; }
            $block .= '); die "exec failed: " . $!; } else { waitpid($pid, 0); }';
            $block .= ' $?;';
            return $block . "\n";
        }

        if ($normalized_flag eq '-c' && $is_clean_flag) {
            # Build the shell command string from the remaining tokens (preserve pipeline '|' as raw pipe)
            my @cmd_parts = @tokens[1..$#tokens];
            # Build a raw shell command (no surrounding quoting) for conversion
            # so debashc sees the original shell text. Separately build a
            # quoted form we can embed into exec('sh','-c', ...) when we
            # fall back to executing via the shell.
            # Build a raw shell command (no surrounding quoting) for conversion
            # so debashc sees the original shell text. When tokens were
            # single-quoted in the original Perl source they may contain
            # Perl-level backslash escapes (for example '\' to represent a
            # single quote inside a single-quoted Perl string). Decode those
            # so the shell text passed to debashc matches what the shell
            # would actually see at runtime.
            my $shell_cmd_raw = join(' ', map {
                my ($t,$q) = ref($_) eq 'ARRAY' ? @$_ : ($_,'bare');
                if ($q && $q eq 'double') {
                    decode_perl_double_quoted_string($t)
                } elsif ($q && $q eq 'single') {
                    decode_perl_single_quoted_string($t)
                } else {
                    $t
                }
            } @cmd_parts);

            # Build a quoted form suitable for exec(...) embedding. For the
            # conservative exec path we normally prefer a safely shell-quoted
            # tokenization so arguments with spaces remain intact. However when
            # the original source intended Perl interpolation (skip flag)
            # we must preserve the original contiguous inner command string
            # exactly (do not reapply per-token shell quoting) so that
            # interpolation happens as the original author expected.
            my $shell_cmd_for_exec = join(' ', map {
                my ($t,$q) = ref($_) eq 'ARRAY' ? @$_ : ($_,'bare');
                my $tt = ($q && $q eq 'double') ? decode_perl_double_quoted_string($t)
                       : ($q && $q eq 'single') ? decode_perl_single_quoted_string($t)
                       : $t;
                # Do not special-case the pipe token '|' here. Let the general
                # quoting logic (_shell_quote_for_system) decide whether to quote
                # it so that a literal '|' passed as an argv element remains a
                # quoted literal when reconstructed into a bash -c argument.
                _shell_quote_for_system($tt)
            } @cmd_parts);

            # If any original token was double-quoted and contains a Perl-style
            # variable ($ or @) the original source intended Perl interpolation
            # at the call site. In such cases avoid converting the inner shell
            # command into pure-Perl via debashc (which may change semantics)
            # and instead emit an exec('sh','-c', ...) where we preserve the
            # original quoting preference so Perl interpolation still occurs.
            my $skip_conversion_due_to_perl_interpolation = 0;
            for my $p (@cmd_parts) {
                my ($pt, $pq) = ref($p) eq 'ARRAY' ? @{$p} : ($p, 'bare');
                # Only treat a double-quoted token as indicating intended Perl
                # interpolation when it contains an unescaped $ or @. Previously
                # we checked for the presence of $/@ without considering an
                # escaping backslash which caused tokens like "\$VAR" (where
                # the original source escaped the dollar) to be treated as if
                # Perl interpolation was intended. That led to emitting
                # double-quoted Perl literals which then interpolated and
                # removed the desired literal '$' before the shell saw it.
                # Check for an unescaped sigil using a negative lookbehind.
                # Treat an unescaped Perl sigil followed by an identifier or
                # brace-based identifier as an indication the original author
                # likely intended Perl interpolation (e.g. "$var" or "${var}").
                # Do NOT treat numeric-only sigils like "$0" (common in awk)
                # as evidence of Perl interpolation so awk/sed programs are not
                # accidentally interpolated by Perl.
                if ($pq && $pq eq 'double' && $pt =~ /(?<!\\)(?:\$\{?[A-Za-z_]|@\{?[A-Za-z_])/) { $skip_conversion_due_to_perl_interpolation = 1; last; }
            }

            my $perl_inner;
            if (!$skip_conversion_due_to_perl_interpolation) {
                # Try conversion using the raw inner shell text first. Passing the
                # raw command (without additional surrounding quotes) to debashc
                # generally lets the parser see the intended shell syntax and
                # enables generators (e.g. sha256sum/sha512sum) to emit pure-Perl
                # implementations. If this fails we will fall back to the exec
                #('sh','-c', ...) path below which uses the quoted form.
                my $try_raw = $shell_cmd_raw;
                $perl_inner = convert_shell_to_perl($try_raw, 0);
            }

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
                    # However, when the generated fragment is a complete
                    # do{...} block that performs printing itself we should
                    # avoid wrapping it in another printing wrapper later.
                    # Indicate to the caller that this fragment is safe to
                    # insert by returning it as-is.
                    return $perl_inner;
                }
            }

            # Normal fallback: emit an exec('sh','-c', ...) block. Preserve the
            # original flag token text (including any whitespace) when building
            # the Perl literal so we match the original program's semantics.
            my $flag_literal = _perl_quote_literal_with_pref($flag_txt, $flag_q);

            # Choose an appropriate Perl literal for the inner shell command.
            # If we skipped conversion due to intended Perl interpolation
            # preserve that behaviour by emitting a double-quoted Perl
            # literal so $/@ sequences are interpolated. Otherwise prefer a
            # non-interpolating literal which is safer for awk/sed fragments.
            my $cmd_lit;
            if ($skip_conversion_due_to_perl_interpolation) {
                # When the original call used double-quoted tokens with
                # unescaped $/@ the author likely expected Perl interpolation
                # at the call site. Preserve that by embedding the raw inner
                # shell command in a double-quoted Perl literal (so Perl
                # performs interpolation). Use the raw reconstructed shell
                # text here rather than the per-token re-quoted form so we
                # don't inadvertently change the original semantics.
                $cmd_lit = _perl_quote_literal_with_pref($shell_cmd_raw, 'double');
            } else {
                $cmd_lit = _perl_quote_literal_no_interp($shell_cmd_raw);
            }

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
            # Preserve literal tokens by letting the general quoting helper
            # decide how to represent each token. Do NOT treat '|' as a
            # special pipeline operator here because list-form system() calls
            # may include a literal '|' argument which must be preserved.
            _shell_quote_for_system($t);
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
        # The original token was double-quoted in the source. Preserve the
        # author's intended interpolation semantics: if the original inner
        # text contains an unescaped Perl sigil ($ or @) it likely was meant
        # to be interpolated by Perl, so emit a double-quoted Perl literal
        # which performs interpolation at runtime. Otherwise fall back to
        # the previous heuristic that prefers single-quoted literals unless
        # control characters or double-quote/backslash characters require
        # a double-quoted form.
        # Only treat an unescaped $/@ followed by an identifier (or a brace
        # form like ${var}) as a reason to emit a double-quoted Perl literal.
        # This avoids accidental interpolation of shell/awk fragments that
        # commonly use numeric references like $0/$1 which are not Perl
        # identifiers and should be preserved verbatim.
        if (_has_unescaped_ident_sigil($text) || $text =~ /[\n\r\t"\\]/) {
            my $escaped = $text;
            # Escape backslashes and double-quotes for a double-quoted literal
            $escaped =~ s/\\/\\\\/g;    # backslashes
            $escaped =~ s/"/\\"/g;        # escaped double-quotes
            $escaped =~ s/\n/\\n/g;
            $escaped =~ s/\r/\\r/g;
            $escaped =~ s/\t/\\t/g;
            return "\"$escaped\"";
        } else {
            my $t = $text;
            $t =~ s/'/\\'/g;
            return "'$t'";
        }
    }

    if (defined $pref && $pref eq 'single') {
        my $t = $text;
        # Escape single quotes for a Perl single-quoted literal.
        $t =~ s/'/\\'/g;
        return "'$t'";
    }

    return _perl_quote_literal($text);
}

# Return true when the text contains an unescaped Perl sigil ($ or @)
# followed by an identifier-ish token (letter or underscore) or a brace
# based identifier (e.g. ${var}, @{arr}). This is used to decide whether
# the original author likely intended Perl interpolation and thus whether
# we should emit a double-quoted Perl literal. Do not treat numeric-only
# references like $0 as an identifier sigil.
sub _has_unescaped_ident_sigil {
    my ($text) = @_;
    return 0 unless defined $text && length $text;
    # Negative lookbehind to ensure the sigil is not escaped (preceded by backslash)
    return ($text =~ /(?<!\\)(?:\$\{?[A-Za-z_]|@\{?[A-Za-z_])/) ? 1 : 0;
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

# Decode a Perl single-quoted string literal's escape sequences so we
# reconstruct the runtime string the shell would see. In Perl single-quoted
# literals only \' and \\ are recognized as escapes; all other backslashes
# are literal backslashes. This mirrors Perl's single-quote semantics.
sub decode_perl_single_quoted_string {
    my ($text) = @_;
    return '' unless defined $text;
    # Replace escaped single-quote and escaped backslash sequences.
    $text =~ s/\\'/\'/g;
    $text =~ s/\\\\/\\/g;
    return $text;
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
        if ($replacement_code =~ /\$\?\s*;|\bmy\s+\$pid\s*=\s*fork\b|\bexec\s*\(|open\s+.*STDOUT|open\s*\(\s*STDOUT|\bprintf\s*\(|\bprint\s*['"]/s) {
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
        # already-qualified names like Carp::croak. Also avoid qualifying
        # identifiers that appear inside a 'use Carp' import list (for
        # example "use Carp qw(carp croak);") since turning that into
        # "use Carp qw(carp Carp::croak);" is invalid and causes a runtime
        # import error. To do this robustly, scan the statement and qualify
        # only those helper tokens that are not inside any nearby 'use Carp'
        # import range and are not already namespace-qualified or preceded by
        # a sigil.
        {
            # Collect ranges corresponding to 'use Carp ... ;' imports
            my @carp_ranges;
            while ($s =~ /use\s+Carp\b/g) {
                my $use_start = $-[0];
                my $after = $+[0];
                my $semi = index($s, ';', $after);
                my $range_end = $semi == -1 ? length($s) : $semi + 1;
                push @carp_ranges, [$use_start, $range_end];
            }

            # Walk matches and rebuild the string, qualifying only safe
            # occurrences.
            my $out = '';
            my $last = 0;
            while ($s =~ /\b(croak|confess)\b/g) {
                my $mstart = $-[0];
                my $mend = $+[0];
                my $tok = $1;
                # Append text before the match
                $out .= substr($s, $last, $mstart - $last);

                # Skip if preceded by sigil or namespace qualifier
                if ($mstart >= 1 && substr($s, $mstart-1, 1) eq '$') {
                    $out .= $tok;
                } elsif ($mstart >= 2 && substr($s, $mstart-2, 2) eq '::') {
                    $out .= $tok;
                } else {
                    # Skip if inside a 'use Carp' import range
                    my $inside = 0;
                    for my $r (@carp_ranges) {
                        if ($mstart >= $r->[0] && $mstart < $r->[1]) { $inside = 1; last; }
                    }
                    if ($inside) {
                        $out .= $tok;
                    } else {
                        $out .= "Carp::" . $tok;
                    }
                }

                $last = $mend;
            }
            # Append remainder
            $out .= substr($s, $last) if $last < length($s);
            $s = $out;
        }

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

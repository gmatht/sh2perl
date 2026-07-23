#!/usr/bin/env perl
use strict;
use warnings;

# Scan generated Perl files for qx{} and system() calls with known builtins.
# Exit code = number of violations found.

my @builtins = qw(
    find ls grep sed awk sort uniq head tail cat echo printf
    touch mkdir rmdir rm cp mv chmod chown ln basename dirname
    date sleep wc kill ps cd pwd perl comm cut
    whoami uname hostname bc
);

# Read exemptions from allowed_qx_calls.txt (same file the Rust check uses).
# Each line is a shell-command prefix that is allowed to use qx{}/system().
my @exemptions;
if (open my $fh, '<', '../allowed_qx_calls.txt') {
    while (<$fh>) {
        chomp;
        s/#.*//;
        next if /^\s*$/;
        push @exemptions, $_;
    }
    close $fh;
}

my $is_exempt = sub {
    my ($cmd) = @_;
    for my $pat (@exemptions) {
        return 1 if $cmd =~ /^\Q$pat\E/;
    }
    return 0;
};

sub check_builtins_in_cmd {
    my ($cmd) = @_;
    (my $check = $cmd) =~ s/<\([^)]*\)//g;
    $check =~ s/>\([^)]*\)//g;
    for my $b (@builtins) {
        if ($check =~ /\b\Q$b\E\b/) {
            return $b;
        }
    }
    return undef;
}

sub check_call_args_for_bash_c {
    my ($file_basename, @quoted_args) = @_;
    for my $i (0 .. $#quoted_args - 2) {
        if (($quoted_args[$i] eq 'bash' || $quoted_args[$i] eq 'sh')
            && $quoted_args[$i+1] eq '-c'
            && defined $quoted_args[$i+2])
        {
            my $inner = $quoted_args[$i+2];
            next if $is_exempt->($inner);
            my $b = check_builtins_in_cmd($inner);
            if (defined $b) {
                return "  FAIL: $file_basename.sh [perl] - bash/sh -c wrapping builtin '$b'\n";
            }
        }
    }
    return '';
}

# Extract all quoted string values from a code region, handling
# single-quoted ('...'), double-quoted ("..." with escaped quotes),
# and q{...} literals.  For double-quoted strings, backslash-escaped
# quotes (\") are treated as part of the string content.
sub extract_all_quoted_strings {
    my ($text) = @_;
    my @args;
    # Single-quoted: '...'
    while ($text =~ /'([^']*)'/g) {
        push @args, $1;
    }
    # Double-quoted: "..." with support for escaped \"
    while ($text =~ /"((?:[^"\\]|\\.)*)"/g) {
        my $val = $1;
        $val =~ s/\\(.)/$1/g;  # unescape
        push @args, $val;
    }
    # q{...} literals
    while ($text =~ /q\{([^}]*)\}/g) {
        push @args, $1;
    }
    return @args;
}

my $violations = 0;

for my $file (@ARGV ? @ARGV : glob('examples.out/*.pl')) {
    next unless -f $file;
    open my $fh, '<', $file or next;
    my $code = do { local $/; <$fh> };
    close $fh;

    my $basename = (split '/', $file)[-1];
    $basename =~ s/\.sh\.pl$//;

    # Pattern 1: direct qx{builtin ...}
    while ($code =~ /qx\{([^}]*)\}/g) {
        my $qx_body = $1;
        next if $qx_body =~ /^\$/;
        my $check_cmd = $qx_body;
        if ($check_cmd =~ /^bash -c (["\x27])(.*)\1\s*/s) {
            $check_cmd = $2;
        }
        next if $is_exempt->($check_cmd);
        my $b = check_builtins_in_cmd($check_cmd);
        if (defined $b) {
            print "  FAIL: $basename.sh [perl] - QX violation: qx{} call with builtin '$b'\n";
            $violations++;
        }
    }

    # Pattern 2: qx{$var} where var was assigned a command string containing a builtin.
    while ($code =~ /qx\{(\$\w+)\}/g) {
        my $var = $1;
        my $pos = pos($code);
        my $before = substr($code, 0, $pos);
        my $last_assign = '';
        while ($before =~ /my\s+\Q$var\E\s*=\s*(?:q\{([^}]*)\}|"((?:[^"\\]|\\.)*)"|\x27([^\x27]*)\x27)/sg) {
            my $val = $+;
            if (defined $3) {
                $last_assign = $3;
            } else {
                $last_assign = $val;
                $last_assign =~ s/\\(.)/$1/g;
            }
        }
        next if $last_assign eq '';
        my $check_cmd = $last_assign;
        if ($check_cmd =~ /^bash -c (["\x27])(.*)\1\s*$/s) {
            $check_cmd = $2;
        } elsif ($check_cmd =~ /^bash -c (\S+)\s*$/) {
            $check_cmd = $1;
        }
        next if $is_exempt->($check_cmd);
        my $b = check_builtins_in_cmd($check_cmd);
        if (defined $b) {
            print "  FAIL: $basename.sh [perl] - QX violation: qx{$var} where $var contains builtin '$b'\n";
            $violations++;
        }
    }

    # Pattern 3: system('builtin ...') / system("builtin ...") / system "builtin ..."
    # Handles both system("cmd") and system "cmd" (paren or space).
    while ($code =~ /system\s*(?:\(\s*['"]\s*|['"]\s*)([^'"]+)['"]/g) {
        my $system_body = $1;
        next if $is_exempt->($system_body);
        my $b = check_builtins_in_cmd($system_body);
        if (defined $b) {
            print "  FAIL: $basename.sh [perl] - SYSTEM violation: system() call with builtin '$b'\n";
            $violations++;
        }
    }

    # Pattern 3b: multi-argument system('bash', '-c', 'cmd') etc.
    while ($code =~ /system\s*\((.*?)\)/gs) {
        my $call_args_str = $1;
        next if $call_args_str =~ /^\s*['"]/;
        my @all_quoted = extract_all_quoted_strings($call_args_str);
        next if @all_quoted < 3;
        my $msg = check_call_args_for_bash_c($basename, @all_quoted);
        if ($msg) {
            print $msg;
            $violations++;
        }
    }

    # Pattern 4: open3(..., 'bash', '-c', 'cmd') etc.
    while ($code =~ /open3\s*\((.*?)\)/gs) {
        my $call_args_str = $1;
        my @all_quoted = extract_all_quoted_strings($call_args_str);
        next if @all_quoted < 3;

        my $msg = check_call_args_for_bash_c($basename, @all_quoted);
        if ($msg) {
            print $msg;
            $violations++;
            next;
        }

        # Direct open3 call - find first non-redirect quoted arg
        my $prog = '';
        for my $q (@all_quoted) {
            next if $q eq '>&STDERR' || $q eq '&STDERR' || $q eq '' || $q eq '&1';
            $prog = $q;
            last;
        }
        next if $prog eq '';
        next if $is_exempt->($prog);
        my $b = check_builtins_in_cmd($prog);
        if (defined $b) {
            print "  FAIL: $basename.sh [perl] - OPEN3 violation: open3() with builtin '$b'\n";
            $violations++;
        }
    }

    # Pattern 5: exec('builtin') / exec 'builtin' (single command)
    while ($code =~ /exec\s*(?:\(\s*['"]\s*|['"]\s*)(\w+)(?:\s*['"]|['"]\s*\))/g) {
        my $exec_cmd = $1;
        next if $is_exempt->($exec_cmd);
        my $b = check_builtins_in_cmd($exec_cmd);
        if (defined $b) {
            print "  FAIL: $basename.sh [perl] - EXEC violation: exec() with builtin '$b'\n";
            $violations++;
        }
    }

    # Pattern 5b: exec 'bash', '-c', 'cmd' (with or without parens)
    while ($code =~ /exec\s*(?:\(\s*|)['"](bash|sh)['"]\s*,\s*['"]-c['"]\s*,\s*(['"])(.*?)\2/gs) {
        my $inner = $3;
        next if $is_exempt->($inner);
        my $b = check_builtins_in_cmd($inner);
        if (defined $b) {
            print "  FAIL: $basename.sh [perl] - EXEC violation: exec bash/sh -c wrapping builtin '$b'\n";
            $violations++;
        }
    }
}

exit $violations;

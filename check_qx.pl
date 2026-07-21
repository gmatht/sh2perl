#!/usr/bin/env perl
use strict;
use warnings;

# Scan generated Perl files for qx{} and system() calls with known builtins.
# Exit code = number of violations found.

my @builtins = qw(
    find ls grep sed awk sort uniq head tail cat echo printf
    touch mkdir rmdir rm cp mv chmod chown ln basename dirname
    date sleep wc kill ps cd pwd perl
);

# Read exemptions from allowed_qx_calls.txt (same file the Rust check uses).
# Each line is a shell-command prefix that is allowed to use qx{}/system().
my @exemptions;
if (open my $fh, '<', 'allowed_qx_calls.txt') {
    while (<$fh>) {
        chomp;
        s/#.*//;    # strip comments
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

my $violations = 0;

for my $file (@ARGV ? @ARGV : glob('examples.out/*.pl')) {
    next unless -f $file;
    open my $fh, '<', $file or next;
    my $code = do { local $/; <$fh> };
    close $fh;

    my $basename = (split '/', $file)[-1];
    $basename =~ s/\.sh\.pl$//;

    # Pattern 1: direct qx{builtin ...}
    # First extract the full qx body, then check if it contains a builtin.
    while ($code =~ /qx\{([^}]*)\}/g) {
        my $qx_body = $1;
        next if $qx_body =~ /^\$/;  # skip variable indirection (handled in Pattern 2)
        next if $is_exempt->($qx_body);
        for my $b (@builtins) {
            if ($qx_body =~ /\b\Q$b\E\b/) {
                print "  FAIL: $basename.sh [perl] — QX violation: qx{} call with builtin '$b'\n";
                $violations++;
                last;
            }
        }
    }

    # Pattern 2: qx{\$var} where var was assigned a command string containing a builtin.
    # Search backwards from the qx{} call to find the MOST RECENT assignment to that
    # variable, so variable reuse (e.g. \$command for both \`ls -la\` and \`find ...\`) is
    # handled correctly.
    while ($code =~ /qx\{(\$\w+)\}/g) {
        my $var = $1;
        my $pos = pos($code);
        my $before = substr($code, 0, $pos);
        # Find the last (most recent) assignment to this variable before the qx{} call
        my $last_assign = '';
        while ($before =~ /my\s+\Q$var\E\s*=\s*(?:q\{([^}]*)\}|"([^"]*)"|'([^']*)')/sg) {
            $last_assign = $+;
        }
        next if $last_assign eq '';
        next if $is_exempt->($last_assign);
        for my $b (@builtins) {
            if ($last_assign =~ /\b\Q$b\E\b/) {
                my $line_num = ($before =~ tr/\n//) + 1;
                print "  FAIL: $basename.sh [perl] — QX violation: qx{$var} where $var contains builtin '$b'\n";
                $violations++;
                last;
            }
        }
    }

    # Pattern 3: system('builtin ...') or system("builtin ...")
    # Extract the full command string and check exemptions.
    while ($code =~ /system\s*['"]\s*([^'"]+)['"]/g) {
        my $system_body = $1;
        next if $is_exempt->($system_body);
        for my $b (@builtins) {
            if ($system_body =~ /\b\Q$b\E\b/) {
                print "  FAIL: $basename.sh [perl] — SYSTEM violation: system() call with builtin '$b'\n";
                $violations++;
                last;
            }
        }
    }

    # Pattern 4: open3(..., 'builtin', ...) — direct system call via IPC::Open3
    while ($code =~ /open3\s*\([^)]*['"]\s*([^'",)]+)['"],?[^)]*\)/g) {
        my $open3_cmd = $1;
        next if $is_exempt->($open3_cmd);
        for my $b (@builtins) {
            if ($open3_cmd =~ /\b\Q$b\E\b/) {
                print "  FAIL: $basename.sh [perl] — OPEN3 violation: open3() with builtin '$b'\n";
                $violations++;
                last;
            }
        }
    }

    # Pattern 5: exec('builtin', ...) — replaces Perl process with shell command
    while ($code =~ /exec\s*['"]\s*(\w+)['"]/g) {
        my $exec_cmd = $1;
        next if $is_exempt->($exec_cmd);
        for my $b (@builtins) {
            if ($exec_cmd =~ /\b\Q$b\E\b/) {
                print "  FAIL: $basename.sh [perl] — EXEC violation: exec() with builtin '$b'\n";
                $violations++;
                last;
            }
        }
    }
}

exit $violations;

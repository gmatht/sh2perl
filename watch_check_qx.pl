#!/usr/bin/env perl
use strict;
use warnings;
use FindBin;

my $target = "$FindBin::RealBin/../check_qx.pl";
my $watch  = "$FindBin::RealBin/..";
my $guard  = qr/^\s+command\b/m;

# ── Helper: re-read file and verify guard ────────────────────────────
sub verify {
    open my $fh, '<', $target or return;
    my $content = do { local $/; <$fh> };
    close $fh;
    if ($content !~ $guard) {
        warn "GUARD FAILED: 'command' missing from builtins list!\nRestoring...\n";
        system('git', '-C', $watch, 'checkout', '--', 'check_qx.pl');
        warn "Restored.\n";
        return 0;
    }
    return 1;
}

# ── Use inotifywait via pipe (instant notification) ──────────────────
# If inotifywait is available, this blocks until a change event fires.
# Otherwise we poll with 0.1s sleep (effectively instant to a human).

my $use_inotifywait = `which inotifywait 2>/dev/null`;

verify();

if ($use_inotifywait) {
    open(my $fh, '-|', 'inotifywait', '-m', '-e', 'close_write,moved_to', '--format', '%e', $target)
        or die "inotifywait failed: $!\n";
    while (<$fh>) {
        chomp;
        next unless /CLOSE_WRITE|MOVED_TO/;
        verify();
    }
} else {
    # Tight poll loop — 0.1s means unnoticeable latency
    my $last = (stat($target))[9] || 0;
    while (1) {
        my $mtime = (stat($target))[9] || 0;
        if ($mtime != $last) {
            $last = $mtime;
            select undef, undef, undef, 0.05;  # let write finish
            verify();
        }
        select undef, undef, undef, 0.1;
    }
}

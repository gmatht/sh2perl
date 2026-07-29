#!/usr/bin/env perl
use strict;
use warnings;
use FindBin;
use File::Basename;

my $target_file = "$FindBin::RealBin/../check_qx.pl";
my $watch_dir   = dirname($target_file);

# Patterns that MUST be present in check_qx.pl
my @guard_patterns = (
    qr/^\s+command\b/m,         # in the qw() builtins list
    qr/eq\s+'command'/,         # hard-coded string check
);

# Use inotify if available, otherwise poll
my $use_inotify = eval { require Linux::Inotify2; 1 };

print "Watching $target_file for tampering...\n";
print "Guards: " . scalar(@guard_patterns) . " patterns\n";
print "Using inotify\n" if $use_inotify;
print "Polling every 2 seconds\n" unless $use_inotify;

my $last_mtime = (stat $target_file)[9] // 0;

sub check_file {
    open my $fh, '<', $target_file or return;
    my $content = do { local $/; <$fh> };
    close $fh;

    for my $pat (@guard_patterns) {
        if ($content !~ $pat) {
            warn "GUARD FAILED: pattern " . (qr//) . " not found\n";
            warn "Restoring $target_file from git...\n";
            system('git', '-C', $watch_dir, 'checkout', '--', 'check_qx.pl');
            warn "Restored.\n";
            return 0;
        }
    }
    return 1;
}

if ($use_inotify) {
    my $inotify = Linux::Inotify2->new();
    $inotify->watch($target_file, IN_MODIFY|IN_CLOSE_WRITE, sub {
        my $ev = shift;
        select undef, undef, undef, 0.1;  # let write finish
        check_file();
    });
    1 while $inotify->poll;
} else {
    # Polling fallback
    while (1) {
        my $mtime = (stat $target_file)[9] // 0;
        if ($mtime > $last_mtime) {
            $last_mtime = $mtime;
            select undef, undef, undef, 0.1;
            check_file();
        }
        sleep 2;
    }
}

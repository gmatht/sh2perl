#!/usr/bin/env perl
use strict;
use warnings;
use FindBin;

my $target_file = "$FindBin::RealBin/../check_qx.pl";
my $watch_dir   = "$FindBin::RealBin/..";

# Patterns that MUST be present in check_qx.pl
my @guard_patterns = (
    qr/^\s+command\b/m,         # in the qw() builtins list
    qr/eq\s+'command'/,         # hard-coded string check
);

print "Watching $target_file for tampering...\n";
print "Guards: " . scalar(@guard_patterns) . " patterns\n";
print "Polling every 2 seconds\n";

my $last_mtime = (stat $target_file)[9] // 0;

sub check_file {
    open my $fh, '<', $target_file or return;
    my $content = do { local $/; <$fh> };
    close $fh;

    for my $pat (@guard_patterns) {
        if ($content !~ $pat) {
            warn "GUARD FAILED: pattern $pat not found in $target_file\n";
            warn "Restoring from git...\n";
            system('git', '-C', $watch_dir, 'checkout', '--', 'check_qx.pl');
            warn "Restored.\n";
            return 0;
        }
    }
    return 1;
}

while (1) {
    my $mtime = (stat $target_file)[9] // 0;
    if ($mtime > $last_mtime) {
        $last_mtime = $mtime;
        select undef, undef, undef, 0.1;
        check_file();
    }
    sleep 2;
}

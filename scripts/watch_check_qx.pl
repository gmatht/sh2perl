#!/usr/bin/env perl
use strict;
use warnings;
use Cwd qw(abs_path);
use File::Basename qw(dirname);

# Script location — always works regardless of how the script is invoked
my $script_dir = dirname(abs_path($0));
my $target = "$script_dir/../check_qx.pl";
my $watch  = "$script_dir/..";
my $guard  = qr/^\s+command\b/m;

print "Watching: $target\n";
print "Git dir:  $watch\n";

sub verify {
    open my $fh, '<', $target or do { warn "Cannot read $target: $!\n"; return };
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

verify();

# Use inotifywait if available, otherwise tight poll
my $use_inotifywait = `which inotifywait 2>/dev/null`;

if ($use_inotifywait) {
    open(my $fh, '-|', 'inotifywait', '-m', '-e', 'close_write,moved_to', '--format', '%e', $target)
        or die "inotifywait failed: $!\n";
    while (<$fh>) {
        chomp;
        next unless /CLOSE_WRITE|MOVED_TO/;
        verify();
    }
} else {
    my $last = (stat($target))[9] || 0;
    while (1) {
        my $mtime = (stat($target))[9] || 0;
        if ($mtime != $last) {
            $last = $mtime;
            select undef, undef, undef, 0.05;
            verify();
        }
        select undef, undef, undef, 0.1;
    }
}

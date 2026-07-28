#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $DPKG_ROOT;

$__set_e = 1;
if ((("${DPKG_ROOT:-}" eq q{} && "$1" eq remove) && (-d '/run/systemd/system'))) {
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('deb-systemd-invoke', 'stop', 'hostapd.service') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
        1;
    }
;
}
if ((("${DPKG_ROOT:-}" eq q{} && "$1" eq remove) && (-x "/etc/init.d/hostapd"))) {
        $main_exit_code = system('invoke-rc.d', "--skip-" . "sys" . "tem" . "d-native", 'hostapd', 'stop') >> 8;
    if ($CHILD_ERROR != 0) {
        exit 1;
    }
;
}

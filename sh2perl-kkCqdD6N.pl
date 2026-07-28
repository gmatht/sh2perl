#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $INIT_D_SCRIPT_SOURCED;

if (true ne "$INIT_D_SCRIPT_SOURCED") {

    $INIT_D_SCRIPT_SOURCED = 'true';
    $main_exit_code = system('.', '/lib/init/init-d-script') >> 8;
}
my $DESC = "iSCSI initiator daemon";
my $DAEMON = '/usr/sbin/iscsid';
my $PIDFILE = '/run/iscsid.pid';
my $OMITDIR = '/run/sendsigs.omit.d';

sub do_start_prepare {
if (system('bash', '/usr/lib/open-iscsi/startup-checks.sh') >> 8) {
exit 1;
    }
    return;
}

sub do_start_cleanup {
symlink q{f}, $OMITDIR or warn "symlink failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
    return;
}

sub do_stop_override {
if (((-f '/etc/iscsi/iscsi.initramfs') || !(    do {
        local %ENV = %ENV;
        my $PIDFILE = $PIDFILE;
        my $DAEMON = $DAEMON;
        my $DESC = $DESC;
        my $OMITDIR = $OMITDIR;
        if ((-f '/run/open-iscsi/shutdown-keep-sessions')) {
            "$(cat /run/open-iscsi/shutdown-keep-sessions)" ne q{}
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        q{};
    }))) {
return;
    }
    $main_exit_code = system('do_stop', "\@ARGV") >> 8;
    return;
}

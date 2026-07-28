#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $LXCFS_PID;
my $DEAD;
my $OVS_PID;
my $reason;
my $PID;
my $label;
my $SNAP_COMMON;
my $STATUS;

$__set_e = 1;
# set u not implemented
$ENV{PYTHONPATH} = '/snap/lxd/current/lib/python3/dist-packages/';
$ENV{PYTHONDONTWRITEBYTECODE} = 1;
if ((-d '/sys/kernel/security/apparmor')) {
    $label = (do { my @_qx_cmd = ("cat /proc/self/attr/current 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; });
if (("$label" ne "unconfined" && "${label##*(unconfined)}" ne q{})) {
# Builtin command 'exec' not implemented
    }
}
$reason = "host shutdown";
if (((-s "${SNAP_COMMON}/state") > 0)) {
open STDIN, '<', ${SNAP_COMMON} . "/state" or croak "Cannot read file: $OS_ERROR\n";
$reason = <>;
chomp $reason;
$CHILD_ERROR = defined($reason) ? 0 : 1;
}
else {
    $STATUS = do {
    my $command = 'snap-query 2> /dev/null || true';
    my ($in, $out, $err);
    my $pid = open3($in, $out, $err, 'bash', '-c', $command);
    close $in or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
    close $out or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result;
};
if ("${STATUS}" eq "auto-refresh") {
        $reason = "snap refresh";
}
    else {
        if ("${STATUS}" eq "refresh-snap") {
            $reason = "snap refresh";
}
        else {
            if ("${STATUS}" eq "install-snap") {
                $reason = "snap refresh";
}
            else {
                if ("${STATUS}" eq "remove-snap") {
                    $reason = "snap removal";
                }
            }
        }
    }
}
say "=> Stop reason is: " . ${reason};
if ("${reason}" eq "shutdown") {
exit 0;
}
if (("${reason}" eq "reload" || "${reason}" eq "crashed")) {
exit 0;
}
$ENV{LXD_DIR} = '';
open STDIN, '<', ${SNAP_COMMON} . "/lxd.pid" or croak "Cannot read file: $OS_ERROR\n";
$PID = <>;
chomp $PID;
$CHILD_ERROR = defined($PID) ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
if ("${reason}" eq "snap refresh") {
    say "=> Stopping LXD";
if (("${PID}" ne q{} && !(    do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '0';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
    }))) {
if (my $signal = 'TERM';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}) {
            say "==> Failed to signal LXD to exit";
        }
        $DEAD = q{0};
        my $_;
        for my $_ (do { my $last; $last = '320'; join "\n", 1..$last; }) {
if (            do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '0';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
            }) {
                $DEAD = q{1};
                say "==> Stopped LXD";
last;
            }
require Time::HiRes; Time::HiRes::sleep(q{1});
        }
;
if ("${DEAD}" eq "0") {
            say "==> Forcefully stopping LXD after 5 minutes wait";
if (!(            do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '9';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
            };)) {
                say "==> Stopped LXD";
}
            else {
                say "==> Failed to stop LXD";
            }
        }
    }
exit 0;
}
say "=> Stopping LXD (with instance shutdown)";
open my $fh, '>', "\${SNAP_COMMON} . \"/state\"" or die "${SNAP_COMMON} . "/state": $!\n";
say {*fh} "host-shutdown";
close $fh;
if (("${PID}" ne q{} && !(do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '0';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
}))) {
if (my $signal = '30';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}) {
        say "==> Failed to signal LXD to shutdown";
    }
    $DEAD = q{0};
    for my $_ (do { my $last; $last = '540'; join "\n", 1..$last; }) {
if (        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '0';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
        }) {
            $DEAD = q{1};
            say "==> Stopped LXD";
last;
        }
require Time::HiRes; Time::HiRes::sleep(q{1});
    }
if ("${DEAD}" eq "0") {
        say "==> Forcefully stopping LXD after 9 minutes wait";
if (!(        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '9';
my @pids = (${PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
        };)) {
            say "==> Stopped LXD";
}
        else {
            say "==> Failed to stop LXD";
        }
    }
}
if ((-e "${SNAP_COMMON}/openvswitch/run/ovs-vswitchd.pid")) {
    open STDIN, '<', ${SNAP_COMMON} . "/openvswitch/run/ovs-vswitchd.pid" or croak "Cannot read file: $OS_ERROR\n";
$OVS_PID = <>;
chomp $OVS_PID;
$CHILD_ERROR = defined($OVS_PID) ? 0 : 1;
    if ($CHILD_ERROR != 0) {
        1;
    }
;
if (("${OVS_PID}" ne q{} && !(    do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '0';
my @pids = (${OVS_PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
    }))) {
        do {
            local %ENV = %ENV;
            my $_ = $_;
                say "=> Stopping Open vSwitch";
$__set_e = 1;
$ENV{OVS_LOGDIR} = '';
$ENV{OVS_RUNDIR} = '';
$ENV{OVS_DBDIR} = '';
$ENV{OVS_SYSCONFDIR} = '';
$ENV{OVS_PKGDATADIR} = '';
$ENV{OVS_BINDIR} = '';
$ENV{OVS_SBINDIR} = '';
if (!(                $CHILD_ERROR = 0)) {
                    say "==> Stopped Open vSwitch";
}
                else {
                    say "==> Failed to stop Open vSwitch";
                }
            q{};
        };
    }
}
if ((-e "${SNAP_COMMON}/lxcfs.pid")) {
    open STDIN, '<', ${SNAP_COMMON} . "/lxcfs.pid" or croak "Cannot read file: $OS_ERROR\n";
$LXCFS_PID = <>;
chomp $LXCFS_PID;
$CHILD_ERROR = defined($LXCFS_PID) ? 0 : 1;
    if ($CHILD_ERROR != 0) {
        1;
    }
;
if (("${LXCFS_PID}" ne q{} && !(    do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '0';
my @pids = (${LXCFS_PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
    }))) {
        say "=> Stopping LXCFS";
if (my $signal = 'TERM';
my @pids = (${LXCFS_PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}) {
            say "==> Failed to signal LXCFS to stop";
        }
        $DEAD = q{0};
        for my $_ (do { my $last; $last = '30'; join "\n", 1..$last; }) {
if (            do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '0';
my @pids = (${LXCFS_PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
            }) {
                $DEAD = q{1};
                say "==> Stopped LXCFS";
last;
            }
require Time::HiRes; Time::HiRes::sleep(q{1});
        }
if ("${DEAD}" eq "0") {
            say "==> Forcefully stopping LXCFS after 30 seconds wait";
if (!(            do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $signal = '9';
my @pids = (${LXCFS_PID});
foreach my $pid (@pids) {
if ($pid =~ /^\\d+$/msx) {
my $result = kill $signal, $pid;
if ($result) {
print "Sent signal $signal to process $pid\n";
} else {
print {*STDERR} "kill: ($pid) - No such process\n";
}
} else {
print {*STDERR} "kill: invalid process id: $pid\n";
}
}
            };)) {
                say "==> Stopped LXCFS";
}
            else {
                say "==> Failed to stop LXCFS";
            }
        }
                do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('fusermount', '-u', ${SNAP_COMMON} . "/var/lib/lxcfs") >> 8;
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
}
say "=> Cleaning up PID files";
unlink({SNAP_COMMON} . "/lxcfs.pid");
unlink({SNAP_COMMON} . "/lxd.pid");
say "=> Cleaning up namespaces";
do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
    $main_exit_code = system('nsenter', '-t', q{1}, '-m', 'umount', '-l', ${SNAP_COMMON} . "/ns") >> 8;
};
if ($CHILD_ERROR != 0) {
    1;
}
say "=> All done";
exit 0;

exit $main_exit_code;

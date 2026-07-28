#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $RSYNC_DEFAULTS_FILE;
my $RSYNC_CONFIG_FILE;
my $RETVAL;
my $RSYNC_PID_FILE;
my $VERBOSE;

$__set_e = 1;
my $DAEMON = '/usr/bin/rsync';
my $RSYNC_ENABLE = 'false';
my $RSYNC_OPTS = q{};
$RSYNC_DEFAULTS_FILE = '/etc/default/rsync';
$RSYNC_CONFIG_FILE = '/etc/rsyncd.conf';
$RSYNC_PID_FILE = '/var/run/rsync.pid';
my $RSYNC_NICE_PARM = q{};
my $RSYNC_IONICE_PARM = q{};
$main_exit_code = system('test', '-x', $DAEMON) >> 8;
if ($CHILD_ERROR != 0) {
    exit 0;
}
$main_exit_code = system('.', '/lib/lsb/init-functions') >> 8;
if (((-s $RSYNC_DEFAULTS_FILE) > 0)) {
    $main_exit_code = system('.', $RSYNC_DEFAULTS_FILE) >> 8;
if ("x$RSYNC_ENABLE" eq 'xtrue' or "x$RSYNC_ENABLE" eq 'xfalse') {
    } elsif ("x$RSYNC_ENABLE" eq 'xinetd') {
        exit 0;
    } elsif (1) {
                $main_exit_code = system('log_failure_msg', "Value of RSYNC_ENABLE in $RSYNC_DEFAULTS_FILE must be either 'true' or 'false';") >> 8;
                $main_exit_code = system('log_failure_msg', "not starting rsync daemon.") >> 8;
        exit 1;
    }
if ("x$ENV{RSYNC_NICE}" =~ /^x\[0-9\]$/msx or "x$ENV{RSYNC_NICE}" =~ /^x1\[0-9\]$/msx) {
                $RSYNC_NICE_PARM = "--nicelevel $ENV{RSYNC_NICE}";
    } elsif ("x$ENV{RSYNC_NICE}" eq 'x') {
    } elsif (1) {
                $main_exit_code = system('log_warning_msg', "Value of RSYNC_NICE in $RSYNC_DEFAULTS_FILE must be a value between 0 and 19 (inclusive);") >> 8;
                $main_exit_code = system('log_warning_msg', "ignoring RSYNC_NICE now.") >> 8;
    }
if ("x$ENV{RSYNC_IONICE}" =~ /^x-c\[123\].*$/msx) {
                $RSYNC_IONICE_PARM = "$ENV{RSYNC_IONICE}";
    } elsif ("x$ENV{RSYNC_IONICE}" eq 'x') {
    } elsif (1) {
                $main_exit_code = system('log_warning_msg', "Value of RSYNC_IONICE in $RSYNC_DEFAULTS_FILE must be -c1, -c2 or -c3;") >> 8;
                $main_exit_code = system('log_warning_msg', "ignoring RSYNC_IONICE now.") >> 8;
    }
}
$ENV{PATH} = '';

sub rsync_start {
if ((!-s "$RSYNC_CONFIG_FILE")) {
        $main_exit_code = system('log_failure_msg', "missing or empty config file $RSYNC_CONFIG_FILE") >> 8;
        $main_exit_code = system('log_end_msg', q{1}) >> 8;
exit 0;
    }
if ((("$RSYNC_IONICE_PARM" ne q{} && (-x '/usr/bin/ionice')) && !(    do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
        $main_exit_code = system('/usr/bin/ionice', "$RSYNC_IONICE_PARM", 'true') >> 8;
    }))) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('/usr/bin/ionice', "$RSYNC_IONICE_PARM", '-p', $$) >> 8;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
    my $rc;
if (!(system('start-stop-daemon', '--start', '--quiet', '--background', '--pidfile', $RSYNC_PID_FILE, '--make-pidfile', $RSYNC_NICE_PARM, '--exec', $DAEMON, '--', '--no-detach', '--daemon', '--config', "$RSYNC_CONFIG_FILE", $RSYNC_OPTS) >> 8)) {
        $rc = q{0};
require Time::HiRes; Time::HiRes::sleep(q{1});
if (        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $signal = '0';
my @pids = (do { my $cat_chunk = q{}; if ( open my $fh, '<', $RSYNC_PID_FILE ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $RSYNC_PID_FILE . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
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
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        }) {
            $main_exit_code = system('log_failure_msg', "rsync daemon failed to start") >> 8;
            $rc = q{1};
        }
}
    else {
        $rc = q{1};
    }
;
if (($rc == 0)) {
        $main_exit_code = system('log_end_msg', q{0}) >> 8;
}
    else {
        $main_exit_code = system('log_end_msg', q{1}) >> 8;
        unlink(RSYNC_PID_FILE);
    }
    return;
}
if ("$_[0]" eq 'start') {
    if (!(    $CHILD_ERROR = 0)) {
        $main_exit_code = system('log_daemon_msg', "Starting rsync daemon", "rsync") >> 8;
if ((((-s $RSYNC_PID_FILE) > 0) && !(        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $signal = '0';
my @pids = (do { my $cat_chunk = q{}; if ( open my $fh, '<', $RSYNC_PID_FILE ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $RSYNC_PID_FILE . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
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
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        }))) {
            $main_exit_code = system('log_progress_msg', "apparently already running") >> 8;
            $main_exit_code = system('log_end_msg', q{0}) >> 8;
exit 0;
        }
        rsync_start();
}
    else {
if (((-s "$RSYNC_CONFIG_FILE") > 0)) {
            if ("$VERBOSE" ne no) {
                                $main_exit_code = system('log_warning_msg', "rsync daemon not enabled in $RSYNC_DEFAULTS_FILE, not starting...") >> 8;
                $CHILD_ERROR = 0;
            } else {
                $CHILD_ERROR = 1;
            }
        }
    }
} elsif ("$_[0]" eq 'stop') {
        $main_exit_code = system('log_daemon_msg', "Stopping rsync daemon", "rsync") >> 8;
        $main_exit_code = system('start-stop-daemon', '--stop', '--quiet', '--oknodo', '--retry', '30', '--pidfile', $RSYNC_PID_FILE) >> 8;
        $RETVAL = "${\($? >> 8)}";
        $main_exit_code = system('log_end_msg', $RETVAL) >> 8;
    if ($RETVAL ne 0) {
exit 1;
    }
        unlink(RSYNC_PID_FILE);
} elsif ("$_[0]" eq 'reload' or "$_[0]" eq 'force-reload') {
        $main_exit_code = system('log_warning_msg', "Reloading rsync daemon: not needed, as the daemon") >> 8;
        $main_exit_code = system('log_warning_msg', "re-reads the config file whenever a client connects.") >> 8;
} elsif ("$_[0]" eq 'restart') {
    # set +e not implemented
    if (!(    $CHILD_ERROR = 0)) {
        $main_exit_code = system('log_daemon_msg', "Restarting rsync daemon", "rsync") >> 8;
if ((((-s $RSYNC_PID_FILE) > 0) && !(        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $signal = '0';
my @pids = (do { my $cat_chunk = q{}; if ( open my $fh, '<', $RSYNC_PID_FILE ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $RSYNC_PID_FILE . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
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
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        }))) {
            $main_exit_code = system('start-stop-daemon', '--stop', '--quiet', '--oknodo', '--retry', '30', '--pidfile', $RSYNC_PID_FILE) >> 8;
}
        else {
            $main_exit_code = system('log_warning_msg', "rsync daemon not running, attempting to start.") >> 8;
            unlink(RSYNC_PID_FILE);
        }
        rsync_start();
}
    else {
if (((-s "$RSYNC_CONFIG_FILE") > 0)) {
            if ("$VERBOSE" ne no) {
                                $main_exit_code = system('log_warning_msg', "rsync daemon not enabled in $RSYNC_DEFAULTS_FILE, not starting...") >> 8;
                $CHILD_ERROR = 0;
            } else {
                $CHILD_ERROR = 1;
            }
        }
    }
} elsif ("$_[0]" eq 'status') {
        $main_exit_code = system('status_of_proc', '-p', $RSYNC_PID_FILE, "$DAEMON", 'rsync') >> 8;
    } elsif (1) {
        say "Usage: /etc/init.d/rsync {start|stop|reload|force-reload|restart|status}";
    exit 1;
}
exit 0;

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);
my $DATE_SNAPSHOT = time;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $MEMTESTER_NUM;
my $RUN_DURATION_TIME_FLAG;
my $RUN_LOOPS_FLAG;
my $MEMTESTERCOPY;
my $MEM_RESERVED;

my $VERSION = "Fri Oct 19 11:56:57 CST 2007";
my $MEMTESTER = '/usr/sbin/memtester';
my $PPIDKILL = $$;
my $SIDKILL = $$;
# Builtin command 'trap' with dynamic handler not supported
# Builtin command 'trap' with dynamic handler not supported
print "Version: ${VERSION}
PID: $$
PPIDKILL: ${PPIDKILL}
SIDKILL: ${PPIDKILL}
";
my $CORE_NUM = do { my $result_0 = qx{bash -c 'grep -i ^processor /proc/cpuinfo | wc -l' }; chomp $result_0; $result_0; };
$MEMTESTERCOPY = $CORE_NUM;
my $MEM_TOTAL_K = do { my @_qx_cmd = (q(awk '/^MemTotal/{print $2}' /proc/meminfo)); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
my $MEM_RESERVE_PERCENTAGE = eval { int(1000*50/1024) } // "";
$MEM_RESERVED = eval { int($MEM_TOTAL_K/1024*$MEM_RESERVE_PERCENTAGE/1000) } // "";
my $MEM_TOTAL_TOBETESTED = eval { int($MEM_TOTAL_K/1024-$MEM_RESERVED) } // "";
my $MEM_PER_COPY = eval { int($MEM_TOTAL_TOBETESTED/$MEMTESTERCOPY) } // "";
my $RUN_DURATION_TIME = q{0};
my $RUN_LOOPS = '-1';
$RUN_DURATION_TIME_FLAG = q{0};
$RUN_LOOPS_FLAG = q{0};
my $DDPERCOPY_TIME = '6s';
my $LOGDIR = '/root/memtester-log-';
use File::Path qw(make_path);
my $err;
if ( !-d $LOGDIR ) {
    make_path( $LOGDIR, { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . $LOGDIR . ": $err->[0]\n";
    }
}

sub show_help {
print "    Version: ${VERSION}
    Usage: $(basename ${0})
    -r Directory: the root location of memtester binary file
    -c NUMBER: the copies of memtester should be run
    -m NUMBER: how many memory should be tested totally (in MB)
    -t TIME:   duration mode, how long will the tests go
    -l NUMBER: loops mode,how many loops will each memtester should go
    The option -t and -l are exclusive, which means tests could work
    only with   1. duration mode or 2. loops mode
    RUN 4 copies memtester with in 24 hours, to test total 4000 MB memory:
        $(basename ${0}) -t 24h -c 4 -m 4000
    RUN 2 copies memtester with in 1 hours, to test total 4000 MB memory:
        $(basename ${0}) -t 1h -c 4 -m 4000
    RUN 4 copies memtester with in 2 loops, to test total 3600 MB memory:
        $(basename ${0}) -l 2 -c 4 -m 3600
    -V/-h/-H: show this info.
";
exit 0;
    return;
}
while ( $main_exit_code = system('getopts', ':c:m:t:l:r:p:hHVvx', 'OPTION') >> 8 ) {
if ($OPTION eq 'c') {
                $MEMTESTERCOPY = $OPTARG;
    } elsif ($OPTION eq 'm') {
                $MEM_TOTAL_TOBETESTED = $OPTARG;
                $MEM_RESERVED = eval { int($MEM_TOTAL_K/1024-$MEM_TOTAL_TOBETESTED) } // "";
    } elsif ($OPTION eq 't') {
                if (do {
if ((0 != ${RUN_LOOPS_FLAG})) {
        say "-t and -l are exclusive.";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
            $CHILD_ERROR == 0
        }) {
            exit 222;
        }
                $RUN_DURATION_TIME = $OPTARG;
                $RUN_DURATION_TIME_FLAG = q{1};
    } elsif ($OPTION eq 'l') {
                if (do {
if (do {
if (do {
if (do {
if ((0 != ${RUN_DURATION_TIME_FLAG})) {
        print "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
    $CHILD_ERROR == 0
}) {
        say "-t and -l are exclusive.";
}
    $CHILD_ERROR == 0
}) {
        show_help();
}
    $CHILD_ERROR == 0
}) {
        print "\n";
}
            $CHILD_ERROR == 0
        }) {
            exit 223;
        }
                $RUN_LOOPS = $OPTARG;
                $RUN_LOOPS_FLAG = q{1};
    } elsif ($OPTION eq 'd') {
                $MEMTESTER = $OPTARG;
                $main_exit_code = system('bash', '/memtester') >> 8;
    } elsif ($OPTION eq 'p') {
                $MEMTESTER = $OPTARG;
    } elsif ($OPTION eq 'V' or $OPTION eq 'h' or $OPTION eq 'H') {
                show_help();
    } elsif ($OPTION eq 'v') {
        # set -v not implemented
    } elsif ($OPTION eq 'x') {
        # set -x not implemented
    } elsif ($OPTION =~ /^.$/msx) {
                say "Error...";
                say "?Unknown args...";
        exit 224;
    } elsif (1) {
    }
}
if (do {
if (do {
if (do {
if (do {
if (do {
if ((0 == ${RUN_DURATION_TIME_FLAG})) {
    (0 == ${RUN_LOOPS_FLAG})
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
    $CHILD_ERROR == 0
}) {
        print "\n";
}
    $CHILD_ERROR == 0
}) {
        say "Please specified which mode should we run... -t or -l";
}
    $CHILD_ERROR == 0
}) {
        show_help();
}
    $CHILD_ERROR == 0
}) {
        print "\n";
}
    $CHILD_ERROR == 0
}) {
    exit 225;
}
$MEM_PER_COPY = eval { int($MEM_TOTAL_TOBETESTED/$MEMTESTERCOPY) } // "";
say "Mem total: " . q{ } . eval { int($MEM_TOTAL_K/1024) } // "" . q{ } . 'MB';
say "Core total: " . q{ } . $CORE_NUM;
say "Memtester copys: " . q{ } . $MEMTESTERCOPY;
say "Mem per copy: " . q{ } . $MEM_PER_COPY;
say "Mem total to used: " . q{ } . $MEM_TOTAL_TOBETESTED . q{ } . 'MB';
if ((${MEM_RESERVED} < 1)) {
    say "Mem reserved: -- No more memory reserved...";
}
else {
    say "Mem reserved: " . q{ } . $MEM_RESERVED . q{ } . 'MB';
}
if ((0 != ${RUN_DURATION_TIME_FLAG})) {
    say "Run within a duration: " . ${RUN_DURATION_TIME};
}
else {
    if ((0 != ${RUN_LOOPS_FLAG})) {
        say "Run within a loop: " . ${RUN_LOOPS};
    }
}
say "Working directory: " . q{ } . $PWD;
say "Memtester: " . q{ } . $MEMTESTER;
say "LOGs directory: " . q{ } . $LOGDIR;
print "\n";
print "Jobs started at date: ";
my $date = do {
require POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime())
} . "\n";
print $date;
print "\n";
if (my $pid = fork()) {
    # Parent process continues
} elsif (defined $pid) {
    # Child process executes the background command
    exec 'bash', '-c', q{: 'Complex command not supported in bash string generation'};
    croak "exec failed: $OS_ERROR\n";
} else {
    die "Cannot fork: $ERRNO\n";
}
print "Waiting (PID: $ENV{$}) for " . ${MEMTESTERCOPY} . "
    memtesters(" . ${MEM_PER_COPY} . "MB for each). ";
if ((0 != ${RUN_DURATION_TIME_FLAG})) {
    print "For time: " . ${RUN_DURATION_TIME} . " ";
}
if ((0 != ${RUN_LOOPS_FLAG})) {
    print "For loops: " . ${RUN_LOOPS} . " ";
}
say "...";
while ( 1 ) {
    $MEMTESTER_NUM = q{0};
    print "{";
while ( ${MEMTESTER_NUM} < ${MEMTESTERCOPY} ) {
        print " " . ${MEMTESTER_NUM} . " ";
if ((0 != ${RUN_DURATION_TIME_FLAG})) {
            $RUN_LOOPS = q{0};
        }
        if (my $pid = fork()) {
            # Parent process continues
        } elsif (defined $pid) {
            # Child process executes the background command
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $LOGDIR
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
                my $tmp = do {
                $CHILD_ERROR = 0;
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            exit(0);
        } else {
            die "Cannot fork: $ERRNO\n";
        }
require Time::HiRes; Time::HiRes::sleep($DDPERCOPY_TIME);
        $MEMTESTER_NUM = do {
    my ($in_5, $out_5);
    my $pid_5 = open3($in_5, $out_5, '>&STDERR', 'expr', $MEMTESTER_NUM, q{+}, q{1});
    close $in_5 or croak 'Close failed: $OS_ERROR';
    my $result_5 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_5> };
    close $out_5 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_5, 0;
    $result_5
};
    }
    print "}";
1 while wait() > -1;
$CHILD_ERROR = $? == -1 ? 0 : $? >> 8;
    if ((0 != ${RUN_LOOPS_FLAG})) {
        last;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
}
print "\n";
print "End of testing(Excution ended)... ";
$main_exit_code = system('pkill', '-9', '-P', $PPIDKILL) >> 8;
my $signal = 'TERM';
my @pids = ($$);
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
say "Finished the memtester";
print "Jobs finished at date: ";
my $date = do {
require POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime())
} . "\n";
print $date;

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = 'realpath-cmdsub.sh';
# set -o nounset not implemented
# set nounset not implemented

sub try {
    my ($file) = @_;
    my $desc = "$_[0]";
# Builtin command 'shift' not implemented
    my @cmd = (@_);
    print "==== $desc ====\n";
printf('  $ ');
    for my $arg (@cmd) {
if ("$arg" =~ {[[:space:]]}) {
printf('\'%s\' ', "$arg");
}
        else {
printf('%s ', "$arg");
        }
    }
    print "\n";
    my $stdout;
    my $stderr;
    my $rc;
    $stdout = do { my @_qx_cmd = ("${cmd}:@ Variable(\"$\", false, None) 2> /tmp/realpath_stderr."); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $rc = $?;
    $stderr = do {
    my $command = 'cat /tmp/realpath_stderr. Variable("$", false, None) 2> /dev/null || true';
    my ($in, $out, $err);
    my $pid = open3($in, $out, $err, 'bash', '-c', $command);
    close $in or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
    close $out or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result;
};
    unlink('/tmp/realpath_stderr.');
    unlink($);
if ("$stdout" ne q{}) {
if ("$stdout" =~ /^.*[$]'\x00'.*$/ms) {
printf("  stdout (NUL\x{2011}terminated): ");
            # Original bash: printf '%s' "$stdout" | od -A n -t x1z
my $output_567 = qx{command printf %s "$stdout" | od -An -t x1z};
chomp $output_567;
print $output_567, "\n";
printf("\n");
}
        else {
printf("  stdout: %s\n", "$stdout");
        }
}
    else {
printf("  stdout: (empty)\n");
    }
if ("$stderr" ne q{}) {
printf("  stderr: %s\n", "$stderr");
    }
printf("  exit code: %d\n\n", "$rc");
    return;
}
try('realpath (default, --physical) on a simple file', 'realpath', '/bin');
try("realpath on a two\x{2011}hop symlink chain: /usr/bin/vi \x{2192} /etc/alternatives/vi \x{2192} /usr/bin/vim.basic", 'realpath', '/usr/bin/vi');
try('realpath on a relative symlink: /usr/local/bin/pi', 'realpath', '/usr/local/bin/pi');
try('realpath on a regular file (no symlinks)', 'realpath', '/etc/hostname');
try('realpath on a directory with .. component', 'realpath', '/tmp/..');
try('--canonicalize-existing on an existing path', 'realpath', '--canonicalize-existing', '/usr/bin/sh');
try('--canonicalize-existing on a path with a missing last component (should fail)', 'realpath', '--canonicalize-existing', '/tmp/no_such_file_xyzzy');
if ($CHILD_ERROR != 0) {
    1;
}
try("--canonicalize-missing on a path with a non\x{2011}existent leaf", 'realpath', '--canonicalize-missing', '/tmp/no_such_file_xyzzy');
try('--canonicalize-missing on a completely imaginary path', 'realpath', '--canonicalize-missing', '/nonexistent/deeply/missing/file');
try("--canonicalize-missing on a path with .. and non\x{2011}existent parts", 'realpath', '--canonicalize-missing', '/tmp/../nonexistent/../foo');
try('--logical: /bin/..  (bin is a symlink, logical resolves .. before following it)', 'realpath', '--logical', '/bin/..');
try('default (--physical) for comparison: /bin/..', 'realpath', '--physical', '/bin/..');
try('--physical: /bin/..  (explicit, same as default)', 'realpath', '--physical', '/bin/..');
try('--strip (no symlink expansion) on /usr/bin/vi', 'realpath', '--strip', '/usr/bin/vi');
try("--strip on /bin  (symlink /bin \x{2192} usr/bin)", 'realpath', '--strip', '/bin');
try('compare: default (--physical) on /bin', 'realpath', '/bin');
try('--relative-to=/usr/bin for /usr/bin/sh', 'realpath', '--relative-to=/usr/bin', '/usr/bin/sh');
try('--relative-to=/tmp for /etc/hostname', 'realpath', '--relative-to=/tmp', '/etc/hostname');
try('--relative-to=/ for /etc/hostname', 'realpath', '--relative-to=/', '/etc/hostname');
try("--relative-base=/etc for /etc/hostname (below /etc \x{2192} relative)", 'realpath', '--relative-base=/etc', '/etc/hostname');
try("--relative-base=/etc for /usr/bin/sh (not below /etc \x{2192} absolute)", 'realpath', '--relative-base=/etc', '/usr/bin/sh');
try('--relative-base=/ with --relative-to=/tmp  (combined)', 'realpath', '--relative-base=/', '--relative-to=/tmp', '/etc/hostname', '/usr/bin/sh');
print "==== --zero with two paths ====\
";
printf("  \$ realpath --zero /bin /usr/bin/sh\n");

my @_qx_cmd_576 = ('command realpath --zero /bin /usr/bin/sh');
${} = do { chomp(my $_r_576 = qx{command $_qx_cmd_576[0]}); $_r_576; };
my $rc_zero = $?;
printf("  (raw output with NULs above; use od to verify):\n");
printf('  ');
# Original bash: realpath --zero /bin /usr/bin/sh | od -A n -t x1z | head -3
my $output_579 = qx{command realpath --zero /bin /usr/bin/sh | od -An -t x1z | head -3};
chomp $output_579;
print $output_579, "\n";
printf("  exit code: %d\n\n", "$rc_zero");
try('--quiet suppresses error message for a truly invalid path', 'realpath', '--quiet', '/nonexistent_dir_xyzzy/foo');
if ($CHILD_ERROR != 0) {
    1;
}
try('without --quiet for comparison (stderr appears)', 'realpath', '/nonexistent_dir_xyzzy/foo');
if ($CHILD_ERROR != 0) {
    1;
}
print "=== All tests completed ===\n";

exit $main_exit_code;

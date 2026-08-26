#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
my $arg;
my $cmd;
my $desc;
my $rc;
my $rc_zero;
my $stderr;
my $stdout;
my @cmd;

# TODO(unsupported): set options
sub try {
    {
        $desc = $_[0];
    };
    shift;
    {
        # TODO(unsupported): local
    };
    say join(' ', "==== " . $desc . " ====");
    print "  \$ ";
    for my $__loop_arg (@cmd) {
        $arg = $__loop_arg;
        if ((($arg) eq ("~[[:space:]]"))) {
            printf("'%s' ", $arg);
        } else {
            printf("%s ", $arg);
        }
    }
    say "";
    {
        {
            $stdout = "";
        };
        {
            $stderr = "";
        };
        {
            $rc = "";
        };
    };
    # TODO(unsupported): capture body stmt
    $stdout = do { my $__qx0 = qx{}; $__qx0 =~ s/\n+$//; $__qx0 };
    $rc = (($? >> 8));
    # TODO(unsupported): capture body stmt
    $stderr = do { my $__qx1 = qx{}; $__qx1 =~ s/\n+$//; $__qx1 };
    unlink "\"/tmp/realpath_stderr.\" . \$\$";
    if ((($stdout) ne "")) {
        if ((($stdout) =~ m{^.*\$'\x00'.*$})) {
            print "  stdout (NUL‑terminated): ";
            qx{'printf' '%s' \$(\$stdout) | 'od' '-A' 'n' '-t' 'x1z'};
            printf("\n", );
        } else {
            printf("  stdout: %s\n", $stdout);
        }
    } else {
        printf("  stdout: (empty)\n", );
    }
    if ((($stderr) ne "")) {
        printf("  stderr: %s\n", $stderr);
    }
    printf("  exit code: %d\n\n", $rc);
}
try("realpath (default, --physical) on a simple file", "realpath", "/bin");
try("realpath on a two‑hop symlink chain: /usr/bin/vi → /etc/alternatives/vi → /usr/bin/vim.basic", "realpath", "/usr/bin/vi");
try("realpath on a relative symlink: /usr/local/bin/pi", "realpath", "/usr/local/bin/pi");
try("realpath on a regular file (no symlinks)", "realpath", "/etc/hostname");
try("realpath on a directory with .. component", "realpath", "/tmp/..");
try("--canonicalize-existing on an existing path", "realpath", "--canonicalize-existing", "/usr/bin/sh");
(((try("--canonicalize-existing on a path with a missing last component (should fail)", "realpath", "--canonicalize-existing", "/tmp/no_such_file_xyzzy")) == 0) || ((system("true")) == 0));
try("--canonicalize-missing on a path with a non‑existent leaf", "realpath", "--canonicalize-missing", "/tmp/no_such_file_xyzzy");
try("--canonicalize-missing on a completely imaginary path", "realpath", "--canonicalize-missing", "/nonexistent/deeply/missing/file");
try("--canonicalize-missing on a path with .. and non‑existent parts", "realpath", "--canonicalize-missing", "/tmp/../nonexistent/../foo");
try("--logical: /bin/..  (bin is a symlink, logical resolves .. before following it)", "realpath", "--logical", "/bin/..");
try("default (--physical) for comparison: /bin/..", "realpath", "--physical", "/bin/..");
try("--physical: /bin/..  (explicit, same as default)", "realpath", "--physical", "/bin/..");
try("--strip (no symlink expansion) on /usr/bin/vi", "realpath", "--strip", "/usr/bin/vi");
try("--strip on /bin  (symlink /bin → usr/bin)", "realpath", "--strip", "/bin");
try("compare: default (--physical) on /bin", "realpath", "/bin");
try("--relative-to=/usr/bin for /usr/bin/sh", "realpath", "--relative-to=/usr/bin", "/usr/bin/sh");
try("--relative-to=/tmp for /etc/hostname", "realpath", "--relative-to=/tmp", "/etc/hostname");
try("--relative-to=/ for /etc/hostname", "realpath", "--relative-to=/", "/etc/hostname");
try("--relative-base=/etc for /etc/hostname (below /etc → relative)", "realpath", "--relative-base=/etc", "/etc/hostname");
try("--relative-base=/etc for /usr/bin/sh (not below /etc → absolute)", "realpath", "--relative-base=/etc", "/usr/bin/sh");
try("--relative-base=/ with --relative-to=/tmp  (combined)", "realpath", "--relative-base=/", "--relative-to=/tmp", "/etc/hostname", "/usr/bin/sh");
say join(' ', "==== --zero with two paths ====");
printf("  \$ realpath --zero /bin /usr/bin/sh\n", );
system("realpath", "--zero", "/bin", "/usr/bin/sh");
$rc_zero = (($? >> 8));
printf("  (raw output with NULs above; use od to verify):\n", );
print "  ";
qx{'realpath' '--zero' '/bin' '/usr/bin/sh' | 'od' '-A' 'n' '-t' 'x1z' | 'head' '-3'};
printf("  exit code: %d\n\n", $rc_zero);
(((try("--quiet suppresses error message for a truly invalid path", "realpath", "--quiet", "/nonexistent_dir_xyzzy/foo")) == 0) || ((system("true")) == 0));
(((try("without --quiet for comparison (stderr appears)", "realpath", "/nonexistent_dir_xyzzy/foo")) == 0) || ((system("true")) == 0));
say join(' ', "=== All tests completed ===");
# 4 construct(s) lowered to TODO markers


#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;
our $CHILD_ERROR;

$PROGRAM_NAME = 'tty-cmdsub.sh';
my $dev;
my $TTY_DEV;

use strict;

sub capture {
    my ($file) = @_;
    my $label = "$_[0]";
# Builtin command 'shift' not implemented
    my $tmp_stdout;
    my $tmp_stderr;
    $tmp_stdout = do { my @_qx_cmd = ('mktemp /tmp/tty_demo_stdout_XXXXXX'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $tmp_stderr = do { my @_qx_cmd = ('mktemp /tmp/tty_demo_stderr_XXXXXX'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$tmp_stdout"
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', "$tmp_stderr" or croak "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $CHILD_ERROR = 0;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    my $ec = $?;
    my $so;
    $so = do { my $cat_chunk = q{}; if ( open my $fh, '<', "$tmp_stdout" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$tmp_stdout" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    my $se;
    $se = do { my $cat_chunk = q{}; if ( open my $fh, '<', "$tmp_stderr" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$tmp_stderr" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    unlink('$tmp_stdout');
    unlink('$tmp_stderr');
    print "--- [" . ${label} . "] ---\n";
    print "  cmd     : \@ARGV\n";
    print "  exitcode: " . ${ec}, "\n";
if ("${so}" ne q{}) {
        print "  stdout  : " . ${so}, "\n";
}
    else {
        print "  stdout  : (empty)\n";
    }
if ("${se}" ne q{}) {
        print "  stderr  : " . ${se}, "\n";
    }
    print "\n";
return $ec;
    return;
}
$TTY_DEV = "";
for my $dev ('/dev/pts/2', '/dev/pts/3', '/dev/pts/4') {
if (-r "$dev") {
        $TTY_DEV = "$dev";
last;
    }
}
$dev = '/dev/pts/4';
if ("$TTY_DEV" eq q{}) {
    for my $dev ('/dev/pts/*') {
if (-r "$dev") {
            $TTY_DEV = "$dev";
last;
        }
    }
}
print "Using terminal device: " . (defined (defined ${TTY_DEV} && ${TTY_DEV} ne q{} ? ${TTY_DEV} : 'NONE') && (defined ${TTY_DEV} && ${TTY_DEV} ne q{} ? ${TTY_DEV} : 'NONE') ne q{} ? (defined ${TTY_DEV} && ${TTY_DEV} ne q{} ? ${TTY_DEV} : 'NONE') : 'NONE'), "\n";
print "\n";
print "============================================================\n";
print " SECTION 1: tty (default) \x{2014} with a real terminal\n";
print "============================================================\n";
if ("$TTY_DEV" ne q{}) {
open STDIN, '<', "$TTY_DEV" or croak "Cannot read file: $OS_ERROR\n";
    capture("01-default-terminal", 'tty');
}
else {
    print "  (skipped \x{2014} no terminal device available)\n";
    print "\n";
}
print "============================================================\n";
print " SECTION 2: tty (default) \x{2014} stdin from /dev/null\n";
print "============================================================\n";
open STDIN, '<', '/dev/null' or croak "Cannot read file: $OS_ERROR\n";
capture("02-not-a-tty", 'tty');
print "============================================================\n";
print " SECTION 3: tty (default) \x{2014} piped input\n";
print "============================================================\n";
capture("03-pipe-notty", 'bash', '-c', "echo \"dummy\" | tty");
print "============================================================\n";
print " SECTION 4: tty -s \x{2014} with a real terminal (silent)\n";
print "============================================================\n";
if ("$TTY_DEV" ne q{}) {
open STDIN, '<', "$TTY_DEV" or croak "Cannot read file: $OS_ERROR\n";
    capture("04-silent-terminal", 'tty', '-s');
}
else {
    print "  (skipped \x{2014} no terminal device available)\n";
    print "\n";
}
print "============================================================\n";
print " SECTION 5: tty -s \x{2014} stdin from /dev/null (silent, not a tty)\n";
print "============================================================\n";
open STDIN, '<', '/dev/null' or croak "Cannot read file: $OS_ERROR\n";
capture("05-silent-notty", 'tty', '-s');
print "============================================================\n";
print " SECTION 6: tty -s \x{2014} piped input (silent, not a tty)\n";
print "============================================================\n";
capture("06-silent-pipe", 'bash', '-c', "echo \"dummy\" | tty -s");
print "============================================================\n";
print " SECTION 7: tty --silent \x{2014} long form, with a real terminal\n";
print "============================================================\n";
if ("$TTY_DEV" ne q{}) {
open STDIN, '<', "$TTY_DEV" or croak "Cannot read file: $OS_ERROR\n";
    capture("07-long-silent", 'tty', '--silent');
}
else {
    print "  (skipped \x{2014} no terminal device available)\n";
    print "\n";
}
print "============================================================\n";
print " SECTION 8: tty --quiet \x{2014} long form, with a real terminal\n";
print "============================================================\n";
if ("$TTY_DEV" ne q{}) {
open STDIN, '<', "$TTY_DEV" or croak "Cannot read file: $OS_ERROR\n";
    capture("08-long-quiet", 'tty', '--quiet');
}
else {
    print "  (skipped \x{2014} no terminal device available)\n";
    print "\n";
}
print "============================================================\n";
print " SECTION 9: tty --silent \x{2014} stdin from /dev/null (not a tty)\n";
print "============================================================\n";
open STDIN, '<', '/dev/null' or croak "Cannot read file: $OS_ERROR\n";
capture("09-long-silent-notty", 'tty', '--silent');
print "============================================================\n";
print " SECTION 10: tty --quiet \x{2014} stdin from /dev/null (not a tty)\n";
print "============================================================\n";
open STDIN, '<', '/dev/null' or croak "Cannot read file: $OS_ERROR\n";
capture("10-long-quiet-notty", 'tty', '--quiet');
print "============================================================\n";
print " SECTION 11: tty (default) \x{2014} inheriting stdin from the script\n";
print "============================================================\n";
capture("11-inherited", 'tty');
print "============================================================\n";
print " SECTION 12: tty -s \x{2014} inheriting stdin from the script\n";
print "============================================================\n";
capture("12-inherited-silent", 'tty', '-s');
print "============================================================\n";
print " SECTION 13: tty -s -s \x{2014} repeated silent flag\n";
print "============================================================\n";
if ("$TTY_DEV" ne q{}) {
open STDIN, '<', "$TTY_DEV" or croak "Cannot read file: $OS_ERROR\n";
    capture("13-double-silent", 'tty', '-s', '-s');
}
else {
    print "  (skipped \x{2014} no terminal device available)\n";
    print "\n";
}
print "\n";
print "All tty demo sections completed.\n";

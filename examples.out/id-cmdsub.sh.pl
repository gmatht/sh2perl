#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;
our $CHILD_ERROR;

use strict;
my $RC = q{0};

sub capture {
    my ($file) = @_;
    my $label = "$_[0]";
# Builtin command 'shift' not implemented
    my $tmp_stdout;
    my $tmp_stderr;
    $tmp_stdout = do { my @_qx_cmd = ('mktemp /tmp/id_demo_stdout_XXXXXX'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $tmp_stderr = do { my @_qx_cmd = ('mktemp /tmp/id_demo_stderr_XXXXXX'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
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
if ("${se}" ne q{}) {
        print "  stderr  : " . ${se}, "\n";
    }
    print "\n";
return $ec;
    return;
}
print "============================================================\n";
print " SECTION 1: 'id' (default \x{2014} current process)\n";
print "============================================================\n";
capture("01-default", 'id');
print "============================================================\n";
print " SECTION 2: id -u  (effective user ID, numeric)\n";
print "============================================================\n";
capture("02-u", 'id', '-u');
print "============================================================\n";
print " SECTION 3: id -g  (effective group ID, numeric)\n";
print "============================================================\n";
capture("03-g", 'id', '-g');
print "============================================================\n";
print " SECTION 4: id -G  (all group IDs, numeric)\n";
print "============================================================\n";
capture("04-G", 'id', '-G');
print "============================================================\n";
print " SECTION 5: id -un  (effective user name)\n";
print "============================================================\n";
capture("05-un", 'id', '-u', q{n});
print "============================================================\n";
print " SECTION 6: id -gn  (effective group name)\n";
print "============================================================\n";
capture("06-gn", 'id', '-g', q{n});
print "============================================================\n";
print " SECTION 7: id -Gn  (all group names)\n";
print "============================================================\n";
capture("07-Gn", 'id', '-G', q{n});
print "============================================================\n";
print " SECTION 8: id -ru  (real user ID, numeric)\n";
print "============================================================\n";
capture("08-ru", 'id', '-r', q{u});
print "============================================================\n";
print " SECTION 9: id -rg  (real group ID, numeric)\n";
print "============================================================\n";
capture("09-rg", 'id', '-r', q{g});
print "============================================================\n";
print " SECTION 10: id -rG  (real group IDs, numeric)\n";
print "============================================================\n";
capture("10-rG", 'id', '-r', q{G});
print "============================================================\n";
print " SECTION 11: id -run  (real user name)\n";
print "============================================================\n";
capture("11-run", 'id', '-r', 'un');
print "============================================================\n";
print " SECTION 12: id -rgn  (real group name)\n";
print "============================================================\n";
capture("12-rgn", 'id', '-r', 'gn');
print "============================================================\n";
print " SECTION 13: id -rGn  (real group names)\n";
print "============================================================\n";
capture("13-rGn", 'id', '-r', 'Gn');
print "============================================================\n";
print " SECTION 14: id -u -n  (separate options, equivalent to -un)\n";
print "============================================================\n";
capture("14-u-n", 'id', '-u', '-n');
print "============================================================\n";
print " SECTION 15: id -r -u  (separate options, equivalent to -ru)\n";
print "============================================================\n";
capture("15-r-u", 'id', '-r', '-u');
print "============================================================\n";
print " SECTION 16: id -G -z  (group IDs delimited by NUL)\n";
print "============================================================\n";
capture("16-Gz", 'bash', '-c', 'id -G -z | cat -v');
print "============================================================\n";
print " SECTION 17: id -Gn -z  (group names delimited by NUL)\n";
print "============================================================\n";
capture("17-Gnz", 'bash', '-c', 'id -Gn -z | cat -v');
print "============================================================\n";
print " SECTION 18: id -un -z  (user name with NUL terminator)\n";
print "============================================================\n";
capture("18-unz", 'bash', '-c', 'id -un -z | cat -v');
print "============================================================\n";
print " SECTION 19: id -Z  (security context \x{2014} may not be available)\n";
print "============================================================\n";
capture("19-Z", 'id', '-Z');
print "============================================================\n";
print " SECTION 20: id root  (default output for user 'root')\n";
print "============================================================\n";
capture("20-root", 'id', 'root');
print "============================================================\n";
print " SECTION 21: id -u root  (numeric UID of root)\n";
print "============================================================\n";
capture("21-u-root", 'id', '-u', 'root');
print "============================================================\n";
print " SECTION 22: id -g root  (numeric GID of root)\n";
print "============================================================\n";
capture("22-g-root", 'id', '-g', 'root');
print "============================================================\n";
print " SECTION 23: id -G root  (all group IDs of root, numeric)\n";
print "============================================================\n";
capture("23-G-root", 'id', '-G', 'root');
print "============================================================\n";
print " SECTION 24: id -un root  (user name of root)\n";
print "============================================================\n";
capture("24-un-root", 'id', '-u', q{n}, 'root');
print "============================================================\n";
print " SECTION 25: id -ru root  (real user ID of root, numeric)\n";
print "============================================================\n";
capture("25-ru-root", 'id', '-r', q{u}, 'root');
print "============================================================\n";
print " SECTION 26: id -a  (compatibility flag \x{2014} same as default)\n";
print "============================================================\n";
capture("26-a", 'id', '-a');
print "\n";
print "All id demo sections completed.\n";



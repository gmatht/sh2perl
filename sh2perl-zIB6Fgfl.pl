#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $BINARYPERM;

$__set_e = 1;
$main_exit_code = system('test', '-f', '/usr/bin/screen') >> 8;
if ($CHILD_ERROR != 0) {
    exit 0;
}
my $SCREENDIR = '/run/screen';
if ("$_[0]" eq 'start') {
    if ((!(system('test', '-L', $SCREENDIR) >> 8) || !(!($main_exit_code = system('test', '-d', $SCREENDIR) >> 8;)))) {
        unlink(SCREENDIR);
        use File::Path qw(make_path);
        my $err;
        if ( mkdir $SCREENDIR ) {
            }
        else {
            croak "mkdir: cannot create directory " . $SCREENDIR . ": File exists\n";
        }
;
do {
    my ($owner, $group) = split /:/, 'root:utmp', 2;
    my $uid = getpwnam($owner);
    my $gid = defined($group) ? getgrnam($group) : -1;
    chown $uid, $gid, ($SCREENDIR) or warn "chown failed: $OS_ERROR\n";
    $CHILD_ERROR = 0;
};
        if ((-x '/sbin/restorecon')) {
                        $main_exit_code = system('/sbin/restorecon', $SCREENDIR) >> 8;
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
    }
        require File::Find;
    File::Find::find(sub {     next unless -e $_;     print "$File::Find::name\n"; }, 'elete');
        $BINARYPERM = do {
    my ($in_2, $out_2);
    my $pid_2 = open3($in_2, $out_2, '>&STDERR', 'stat', '-c', '%a', '/usr/bin/screen');
    close $in_2 or croak 'Close failed: $OS_ERROR';
    my $result_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_2> };
    close $out_2 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_2, 0;
    $result_2
};
    if (($BINARYPERM >= 4000)) {
chmod(oct(q{0755}), ($SCREENDIR)) or warn "chmod failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
}
    else {
        if (($BINARYPERM >= 2000)) {
chmod(oct(q{0775}), ($SCREENDIR)) or warn "chmod failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
}
        else {
chmod(oct('1777'), ($SCREENDIR)) or warn "chmod failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
        }
    }
} elsif ("$_[0]" eq 'stop' or "$_[0]" eq 'restart' or "$_[0]" eq 'reload' or "$_[0]" eq 'force-reload') {
}
exit 0;

exit $main_exit_code;

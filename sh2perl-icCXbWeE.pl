#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$__set_e = 1;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/pam.d/polkit-1', "122-2\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/polkit-1/localauthority.conf.d/50-localauthority.conf', "121\\+compat0.1-1\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/polkit-1/localauthority.conf.d/51', '-d', 'ebian-sudo.conf', "121\\+compat0.1-1\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/polkit-1/localauthority.conf.d/51', '-u', 'buntu-admin.conf', "121\\+compat0.1-1\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/polkit-1/rules.d/40', '-d', 'ebian-sudo.rules', "121\\~", 'polkitd-javascript', '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/polkit-1/rules.d/40', '-u', 'buntu-admin.rules', "121\\~", 'polkitd-javascript', '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/polkit-1/rules.d/50', '-d', 'efault.rules', "121\\~", 'polkitd-javascript', '--', "\@ARGV") >> 8;
if (("$1" eq remove && (-d '/run/systemd/system'))) {
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('systemctl', "--" . "sys" . "tem", 'daemon-reload') >> 8;
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
if ("$1" eq "purge") {
    unlink('/etc/xml/polkitd.xml');
    unlink('/etc/xml/polkitd.xml.old');
}

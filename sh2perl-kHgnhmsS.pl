#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $DPKG_ROOT;

$__set_e = 1;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/default/dirmngr', '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/dirmngr/dirmngr.conf', '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/dirmngr/ldapservers.conf', '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/init.d/dirmngr', '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/logrotate.d/dirmngr', '--', "\@ARGV") >> 8;
if ("$1" eq "purge") {
if (("${DPKG_ROOT:-}" eq q{} && (-x "/usr/bin/deb-systemd-helper"))) {
                do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('deb-systemd-helper', '--user', 'purge', 'dirmngr.socket') >> 8;
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

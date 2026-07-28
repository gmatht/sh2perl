#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$__set_e = 1;
my $CONF = '/etc/fonts/local.conf';
if ("$_[0]" eq 'purge') {
    if (!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
my $_wa0 = 'ucf';
my $which_prog = q{which};
my $_which_out = qx{$which_prog $_wa0};
print $_which_out;
$CHILD_ERROR = $? >> 8;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };)) {
        $main_exit_code = system('ucf', '--purge', $CONF) >> 8;
    }
        unlink(CONF);
        my $c;
    for my $c ($CONFFILES) {
        unlink(CONFDIR);
        unlink('/');
        unlink(c);
    }
    if ( -e "/etc/fonts/conf.d" ) {
        if ( -d "/etc/fonts/conf.d" ) {
            my $err;
            require File::Path;
            File::Path::remove_tree("/etc/fonts/conf.d", {error => \$err});
            if (@{$err}) {
                carp "rm: carping: could not remove ", "/etc/fonts/conf.d", ": $err->[0]\n";
            }
            else {
                            }
        }
        else {
            if ( unlink "/etc/fonts/conf.d" ) {
                            }
            else {
                carp "rm: carping: could not remove ", "/etc/fonts/conf.d",
              ": $OS_ERROR\n";
            }
        }
    }
    else {
        local $CHILD_ERROR = 0;
    }
            do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
rmdir ('/usr/local/share/fonts') or warn "rmdir failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
    };
    if ($CHILD_ERROR != 0) {
        1;
    }
}
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10-antialias.conf', "2.14.1-3ubuntu1\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', q{/etc/fonts/conf.avail/05}, '-r', 'eset-dirs-sample.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/09-autohint-if-no-hinting.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10-autohint.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-h', 'inting-full.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-h', 'inting-medium.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-h', 'inting-none.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-h', 'inting-slight.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-n', 'o-antialias.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-n', 'o-sub-pixel.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-s', 'cale-bitmap-fonts.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-s', 'ub-pixel-bgr.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-s', 'ub-pixel-rgb.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-s', 'ub-pixel-vbgr.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-s', 'ub-pixel-vrgb.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10', '-u', 'nhinted.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/10-yes-antialias.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/11-lcdfilter-default.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/11-lcdfilter-legacy.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/11-lcdfilter-light.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/20', '-u', 'nhint-small-vera.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/25', '-u', 'nhint-nonlatin.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/30-metric-aliases.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/35-lang-normalize.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/40', '-n', 'onlatin.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/45', '-ge', 'neric.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/45-latin.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/48', '-s', 'pacing.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/49', '-s', 'ansserif.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/50', '-u', 'ser.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/51-local.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/53-monospace-lcd-filter.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/60', '-ge', 'neric.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/60-latin.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/65', '-f', 'onts-persian.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/65', '-k', 'hmer.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/65', '-n', 'onlatin.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/69', '-u', 'nifont.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/70', '-f', 'orce-bitmaps.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/70', '-n', 'o-bitmaps.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/70-yes-bitmaps.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/80', '-d', 'elicious.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;
$main_exit_code = system('dpkg-maintscript-helper', 'rm_conffile', '/etc/fonts/conf.avail/90', '-s', 'ynthetic.conf', "2.14.1-3ubuntu3\\~", '--', "\@ARGV") >> 8;

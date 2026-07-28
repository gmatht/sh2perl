#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$__set_e = 1;
if (("$1" eq "configure" || "$1" eq "abort-upgrade")) {
    $main_exit_code = system('update-alternatives', '--install', '/usr/bin/editor', 'editor', '/bin/nano', '40', '--slave', '/usr/share/man/man1/editor.1.gz', 'editor.1.gz', '/usr/share/man/man1/nano.1.gz') >> 8;
    $main_exit_code = system('update-alternatives', '--install', '/usr/bin/pico', 'pico', '/bin/nano', '10', '--slave', '/usr/share/man/man1/pico.1.gz', 'pico.1.gz', '/usr/share/man/man1/nano.1.gz') >> 8;
}

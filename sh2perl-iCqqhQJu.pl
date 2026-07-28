#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

$__set_e = 1;
$main_exit_code = system('dpkg-maintscript-helper', 'symlink_to_dir', '/usr/share/doc/perl-base', 'perl', '5.30.0-1', 'perl-base', '--', "\@ARGV") >> 8;

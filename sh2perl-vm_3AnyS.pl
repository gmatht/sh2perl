#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

$__set_e = 1;
if (("$2" eq "hibernate" || "$2" eq "hybrid-sleep")) {
if ("$_[0]" eq 'pre') {
                $main_exit_code = system('/usr/share/unattended-upgrades/unattended-upgrade-shutdown', '--stop-only') >> 8;
    }
}

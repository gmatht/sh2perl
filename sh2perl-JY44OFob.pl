#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

$__set_e = 1;
if ("$_[0]" eq 'purge') {
    if ( -e "/etc/python3" ) {
        if ( -d "/etc/python3" ) {
            my $err;
            require File::Path;
            File::Path::remove_tree("/etc/python3", {error => \$err});
            if (@{$err}) {
                carp "rm: carping: could not remove ", "/etc/python3", ": $err->[0]\n";
            }
            else {
                            }
        }
        else {
            if ( unlink "/etc/python3" ) {
                            }
            else {
                carp "rm: carping: could not remove ", "/etc/python3",
              ": $OS_ERROR\n";
            }
        }
    }
    else {
        local $CHILD_ERROR = 0;
    }
}

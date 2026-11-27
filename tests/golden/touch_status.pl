#!/usr/bin/env perl
use strict;
use warnings;

do { my $__ok = 1; for my $__t ('status_file') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; };
do { my $__ok = 1; for my $__t ('/no/such/dir/t') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; };
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, ($? >> 8))), "\n");
$? = 0 << 8;

exit $? >> 8;

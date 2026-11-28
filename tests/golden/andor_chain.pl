#!/usr/bin/env perl
use strict;
use warnings;

do { my $__ok = 1; for my $__d ('chain_dir') { my $__acc = $__d =~ m{^/} ? '' : '.'; for my $__c (split m{/+}, $__d) { next if $__c eq '' || $__c eq '.'; $__acc .= "/$__c"; next if -d $__acc; unless (mkdir($__acc)) { $__ok = 0; print STDERR "mkdir: $!\n"; last; } } } $? = ($__ok ? 0 : 1) << 8; };
if ($? == 0) {
    print "made\n";
    $? = 0 << 8;
    $? = 0 << 8;
} else {
    $? = 0 << 8;
}
do { unlink('no_such_file_here'); $? = 0 << 8; };
if (($? >> 8) != 0) {
    print "gone\n";
    $? = 0 << 8;
}
if (chdir("/no/such")) { print "x\n";
$? = 0 << 8; } else { warn "cd: " . "/no/such" . ": $!\n"; $? = 1 << 8; }
print(join(' ', grep { defined($_) && length($_) } split(/\s+/, ($? >> 8))), "\n");
$? = 0 << 8;

exit $? >> 8;

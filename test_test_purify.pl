use strict;
use warnings;
print "Running\n";
open(my $pipe, '-|', 'perl', './test_purify.pl') or die "Cannot run test_purify.pl: $!";
open(my $out, '>', 'purify.out') or die "Cannot open purify.out: $!";
while (my $line = <$pipe>) {
    print $line;
    print $out $line;
}
close($out);
close($pipe);
print "Ran\n";
open(my $fh, '<', 'purify.out') or die "Cannot open purify.out: $!";
my $output = do { local $/; <$fh> };
close($fh);
print "Slurped\n";

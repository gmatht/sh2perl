#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/010_which_basic.pl" }


use strict;
use warnings;
use File::Path qw(make_path remove_tree);

print "=== Example 010: Basic which command ===\n";

sub find_command {
    my ($name, $path_value) = @_; 
    for my $dir (split /:/, $path_value) {
        my $candidate = "$dir/$name";
        return $candidate if -x $candidate;
    }
    return;
}

print "Using a fixed PATH:\n";
my $original_path = $ENV{PATH};
remove_tree('test_which_bin');
make_path('test_which_bin');
for my $cmd (qw(alpha beta gamma)) {
    open(my $fh, '>', "test_which_bin/$cmd") or die "Cannot create file: $!\n";
    print $fh "#!/bin/sh\necho $cmd\n";
    close($fh);
    chmod 0755, "test_which_bin/$cmd";
}
$ENV{PATH} = "test_which_bin:/usr/bin:/bin";
my $which_output = find_command('alpha', $ENV{PATH}) . "\n";
print "which alpha: $which_output";

print "\nwhich with a fixed command set:\n";
for my $cmd (qw(alpha beta gamma)) {
    my $path = find_command($cmd, $ENV{PATH});
    print "$cmd: " . ($path || 'not found') . "\n";
}

print "\nwhich with all matches (-a):\n";
for my $dir (split /:/, $ENV{PATH}) {
    my $candidate = "$dir/alpha";
    print "$candidate\n" if -x $candidate;
}

print "\nwhich with non-existent command:\n";
my $not_found = find_command('nonexistentcommand', $ENV{PATH});
print $not_found ? "Found: $not_found\n" : "Command not found\n";

$ENV{PATH} = $original_path;
remove_tree('test_which_bin');

print "=== Example 010 completed successfully ===\n";

#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== [[ pattern and regex ]]\n";
my $s;
$s = "file.txt";
if ($s =~ /^.*[.]txt$/msx) {
        print 'pattern-match' . "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if ($s =~ /^file[.][a-z]+$/msx) {
        print 'regex-match' . "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
print "== extglob ==\n";
# extglob option enabled
my $f1;
$f1 = "file.js";
my $f2;
$f2 = "thing.min.js";
if ($f1 =~ /^(?!.*[.]min).*[.]js$/msx) {
        print 'f1-ok' . "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
if (!($f2 =~ /^(?!.*[.]min).*[.]js$/msx)) {
        print 'f2-filtered' . "\n";
}
print "== nocasematch ==\n";
# nocasematch option enabled
my $word;
$word = "Foo";
if ($word =~ /foo/msxi) {
        print 'ci-match' . "\n";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}

exit $main_exit_code;

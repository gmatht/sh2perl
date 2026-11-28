#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
$0 = 'tests/tier1/builtin_status.sh';

our $CHILD_ERROR = 0;


use feature 'say';

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

sub __sh_num {
    # bash arithmetic, not Perl's numeric context: unset and empty are 0,
    # and so is anything that does not parse as a number (bash warns for the
    # last case, but the value is 0 either way — the surrounding expression
    # must not inherit undef).
    my $__v = shift();
    return 0 if !defined($__v) || $__v eq q{};
    no warnings qw(numeric);
    return 0 + $__v;
}

my $mk;
my $n;
my $m;

# __SH_SUBSHELL_BEGIN (bash subshell in condition position)
sub __sh_subshell {
    my $code = shift;
    my $fh = select(STDOUT);
    $| = 1;
    select($fh);
    my $pid = fork;
    if (!defined $pid) {
        $code->();
        return $CHILD_ERROR;
    }
    if ($pid == 0) {
        my $ok = eval { $code->(); 1 };
        my $st = $ok ? $CHILD_ERROR : 1;
        exit($st);
    }
    waitpid($pid, 0);
    my $rc = $? >> 8;
    $CHILD_ERROR = $rc;
    $? = $rc << 8;
    return $rc;
}
# __SH_SUBSHELL_END

{
    open(my $__sh2_dup0, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh0, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "$0: line " . __LINE__ . ": cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    open(STDERR, '>&', $__sh2_dupfh0) or die; close($__sh2_dupfh0);
    open(STDERR, '>&', $__sh2_dup0) or die;
}
say "cd-fail rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('d1') { my $__acc = $__d =~ m{^/} ? '' : '.'; for my $__c (split m{/+}, $__d) { next if $__c eq '' || $__c eq '.'; $__acc .= "/$__c"; next if -d $__acc; unless (mkdir($__acc)) { $__ok = 0; print STDERR "mkdir: $!\n"; last; } } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
if (($? >> 8) == 0) {
    if (chdir("d1")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "$0: line " . __LINE__ . ": cd: " . "d1" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
}
say "cd-ok rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup1, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh1, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("/no/such/dir")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "$0: line " . __LINE__ . ": cd: " . "/no/such/dir" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
    open(STDERR, '>&', $__sh2_dupfh1) or die; close($__sh2_dupfh1);
    open(STDERR, '>&', $__sh2_dup1) or die;
}
if ((0+__sh_str($CHILD_ERROR)) == (0+'0')) {
    print "if-bad\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "if-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
print "still-running\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup2, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh2, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { unlink('/no/such/file.txt'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh2) or die; close($__sh2_dupfh2);
    open(STDERR, '>&', $__sh2_dup2) or die;
}
say "rm-missing rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__t ('gone.txt') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
do { unlink('gone.txt'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
say "rm-ok rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup3, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh3, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { require File::Path; File::Path::remove_tree('/no/such/dir'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh3) or die; close($__sh2_dupfh3);
    open(STDERR, '>&', $__sh2_dup3) or die;
}
say "rm-rf rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup4, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh4, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__p ('no-such-file') { unless (unlink($__p)) { $__ok = 0; print STDERR "rm: cannot remove '$__p': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh4) or die; close($__sh2_dupfh4);
    open(STDERR, '>&', $__sh2_dup4) or die;
}
say "rm-noargs rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup5, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh5, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__d ('/no/such/parent/dir') { unless (mkdir($__d)) { $__ok = 0; print STDERR $!{EEXIST} ? "mkdir: $__d: $!\n" : "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh5) or die; close($__sh2_dupfh5);
    open(STDERR, '>&', $__sh2_dup5) or die;
}
say "mkdir-fail rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('made') { unless (mkdir($__d)) { $__ok = 0; print STDERR $!{EEXIST} ? "mkdir: $__d: $!\n" : "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
say "mkdir-ok rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup6, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh6, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__d ('made') { unless (mkdir($__d)) { $__ok = 0; print STDERR $!{EEXIST} ? "mkdir: $__d: $!\n" : "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh6) or die; close($__sh2_dupfh6);
    open(STDERR, '>&', $__sh2_dup6) or die;
}
$mk = $CHILD_ERROR;
say "mkdir-exists rc=" . __sh_str(($mk));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (int(__sh_num($mk)) ne int(0)) {
    print "mkdir-exists-detected\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
do { my $__ok = 1; for my $__t ('newfile') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
say "touch-ok rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup7, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh7, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__t ('/no/such/dir/f') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh7) or die; close($__sh2_dupfh7);
    open(STDERR, '>&', $__sh2_dup7) or die;
}
say "touch-fail rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print "hi\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "echo rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
print("x\n");
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "printf rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "echo-after-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
open(my $__ofh0, '>', '/dev/null') or die "echo: '/dev/null': $ERRNO"; print $__ofh0 q{y
}; close $__ofh0; $? = 0 << 8; $CHILD_ERROR = $? >> 8;say "printf-after-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup8, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh8, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__d ('/no/such/dir') { unless (rmdir($__d)) { $__ok = 0; print STDERR "rmdir: failed to remove '$__d': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh8) or die; close($__sh2_dupfh8);
    open(STDERR, '>&', $__sh2_dup8) or die;
}
say "rmdir-fail rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
do { my $__ok = 1; for my $__d ('rd') { unless (mkdir($__d)) { $__ok = 0; print STDERR $!{EEXIST} ? "mkdir: $__d: $!\n" : "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
do { my $__ok = 1; for my $__d ('rd') { unless (rmdir($__d)) { $__ok = 0; print STDERR "rmdir: failed to remove '$__d': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
say "rmdir-ok rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = (('1' == '1') ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
say "test-true rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = (('1' == '2') ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
say "test-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = ((('abc' =~ /^(?s)a.*$/)) ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
say "dbl-true rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = ((('abc' =~ /^(?s)z.*$/)) ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
say "dbl-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = ((-f '/etc/passwd') ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
say "test-f rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = ((-f '/nope') ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
say "test-f-missing rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "true rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 1 << 8; $CHILD_ERROR = $? >> 8;
say "false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "true-after-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$n = "0";
my $__sh_loop_rc0 = 0;
while (int(__sh_num($n)) < int(2)) {
    $n = int((__sh_num(__sh_num($n)) + __sh_num(1)));
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    $__sh_loop_rc0 = $? >> 8;
}
$? = ($__sh_loop_rc0) << 8; $CHILD_ERROR = $? >> 8;
say "while-body-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$m = "0";
my $__sh_loop_rc1 = 0;
while (!(int(__sh_num($m)) >=int(1))) {
    $m = int((__sh_num(__sh_num($m)) + __sh_num(1)));
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $__sh_loop_rc1 = $? >> 8;
}
$? = ($__sh_loop_rc1) << 8; $CHILD_ERROR = $? >> 8;
say "until-body-true rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    my $__sub_rc = __sh_subshell(sub {
        exit "4";
    });
    $CHILD_ERROR = $__sub_rc;
    $? = $__sub_rc << 8;
}
say "subshell-exit rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (chdir("/tmp")) {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    warn "$0: line " . __LINE__ . ": cd: " . "/tmp" . ": $!\n";
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
}
say "cd-after-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "echo-after-false-2 rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
say "true-after-false-2 rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
$? = (('1' == '1') ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
say "test-after-false rc=" . __sh_str(($CHILD_ERROR));
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
system('bash', '-c', q{'cd' '-' > '/dev/null' 2>&1});
{
    open(my $__sh2_dup9, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh9, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    if (chdir("/no/such/dir")) {
        
    } else {
        warn "$0: line " . __LINE__ . ": cd: " . "/no/such/dir" . ": $!\n";
        
    }
    open(STDERR, '>&', $__sh2_dupfh9) or die; close($__sh2_dupfh9);
    open(STDERR, '>&', $__sh2_dup9) or die;
}
print "still here\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup10, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh10, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { unlink('/no/such/file'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh10) or die; close($__sh2_dupfh10);
    open(STDERR, '>&', $__sh2_dup10) or die;
}
print "still here 2\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup11, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh11, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__d ('/no/such/x') { unless (mkdir($__d)) { $__ok = 0; print STDERR $!{EEXIST} ? "mkdir: $__d: $!\n" : "mkdir: $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh11) or die; close($__sh2_dupfh11);
    open(STDERR, '>&', $__sh2_dup11) or die;
}
print "still here 3\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
{
    open(my $__sh2_dup12, '>&', \*STDERR) or die;
    open(my $__sh2_dupfh12, '>&', \*STDERR) or die;
    open(STDERR, '>', "/dev/null") or die "redirect: $!\n";
    do { my $__ok = 1; for my $__t ('/no/such/x') { if (open(my $__h, '>>', $__t)) { close($__h); } else { $__ok = 0; print STDERR "touch: cannot touch '$__t': $!\n"; } } $? = ($__ok ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8; };
    open(STDERR, '>&', $__sh2_dupfh12) or die; close($__sh2_dupfh12);
    open(STDERR, '>&', $__sh2_dup12) or die;
}
print "still here 4\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (do {
open(my $__sh2_duperr13, '>&', \*STDERR) or die; open(my $__sh2_dupferr13, '>&', \*STDERR) or die; open(STDERR, '>', "/dev/null") or die "redirect: $!\n"; 
    if (chdir("/tmp")) {
        $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
        warn "$0: line " . __LINE__ . ": cd: " . "/tmp" . ": $!\n";
        $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    }
open(STDERR, '>&', $__sh2_dupferr13) or die; close($__sh2_dupferr13); open(STDERR, '>&', $__sh2_duperr13) or die; 
($CHILD_ERROR == 0)
}) {
    print "elif-cd-ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } elsif (__sh_str($CHILD_ERROR) == '0') {
    print "elif-bad\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
$? = ((int(1) == int(2)) ? 0 : 1) << 8; $CHILD_ERROR = $? >> 8;
print "elif-test-ok\n";
$? = 0 << 8; $CHILD_ERROR = $? >> 8;
if (!((__sh_subshell(sub {
exit "1";
}) == 0))) {
    print "bang-subshell ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    } else {
    print "bang-subshell bad\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    }
if (!((__sh_subshell(sub {
exit "0";
}) == 0))) {
    print "bang-subshell2 bad\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
} else {
    print "bang-subshell2 ok\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
$? = (((do {
    $? = 1 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
}) ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) == 0) {
    print "bang-and-statement ran\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
$? = (((do {
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
    ($? >> 8) == 0
}) ? 1 : 0)) << 8; $CHILD_ERROR = $? >> 8;
if (($? >> 8) == 0) {
    print "bang-and-statement2 BAD\n";
    $? = 0 << 8; $CHILD_ERROR = $? >> 8;
}
if (chdir("/tmp")) {
    
} else {
    warn "$0: line " . __LINE__ . ": cd: " . "/tmp" . ": $!\n";
    
}
do { require File::Path; File::Path::remove_tree('builtin-scratch'); $? = 0 << 8; $CHILD_ERROR = $? >> 8; };

exit $? >> 8;

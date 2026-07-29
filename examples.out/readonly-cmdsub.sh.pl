#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;
our $CHILD_ERROR;

$PROGRAM_NAME = 'readonly-cmdsub.sh';
use strict;
my $RC = q{0};

sub capture {
    my ($file) = @_;
    my $label = "$_[0]";
# Builtin command 'shift' not implemented
    my $tmp_stdout;
    my $tmp_stderr;
    $tmp_stdout = do { my @_qx_cmd = ('mktemp /tmp/readonly_demo_stdout_XXXXXX'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $tmp_stderr = do { my @_qx_cmd = ('mktemp /tmp/readonly_demo_stderr_XXXXXX'); chomp(my $result = qx{command $_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$tmp_stdout"
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', "$tmp_stderr" or croak "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $CHILD_ERROR = 0;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    my $ec = $?;
    my $so;
    $so = do { my $cat_chunk = q{}; if ( open my $fh, '<', "$tmp_stdout" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$tmp_stdout" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    my $se;
    $se = do { my $cat_chunk = q{}; if ( open my $fh, '<', "$tmp_stderr" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$tmp_stderr" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
    unlink('$tmp_stdout');
    unlink('$tmp_stderr');
    print "--- [" . ${label} . "] ---\n";
    print "  cmd     : \@ARGV\n";
    print "  exitcode: " . ${ec}, "\n";
if ("${so}" ne q{}) {
        print "  stdout  : " . ${so}, "\n";
    }
if ("${se}" ne q{}) {
        print "  stderr  : " . ${se}, "\n";
    }
    print "\n";
return $ec;
    return;
}
print "============================================================\n";
print " SECTION 1: Basic readonly variable\n";
print "============================================================\n";
for my $file (glob('
        readonly MY_VAR="hello_world"
        echo "MY_VAR=${MY_VAR}"
        # Attempt to change it (will fail, but we capture)
        MY_VAR="changed" 2>/dev/null || echo "Assignment denied: $?"
        echo "MY_VAR after attempted change=${MY_VAR}"
    ')) {
    capture($file, "01-readonly-var", 'bash', '-c');
}
print "============================================================\n";
print " SECTION 2: readonly -p  (print readonly variables)\n";
print "============================================================\n";
capture("02-readonly-p", 'bash', '-c', "\n        readonly FOO=alpha\n        readonly BAR=beta\n        readonly -p | head -5\n    ");
print "============================================================\n";
print " SECTION 3: readonly -f  (readonly functions)\n";
print "============================================================\n";
capture("03-readonly-f", 'bash', '-c', "\n        myfunc() { echo \"Hello from myfunc\"; }\n        readonly -f myfunc\n        myfunc\n        # Attempt to unset the function (will fail)\n        unset -f myfunc 2>/dev/null || echo \"unset -f denied\"\n        # Verify function still exists\n        myfunc\n    ");
print "============================================================\n";
print " SECTION 4: readonly -a  (indexed array)\n";
print "============================================================\n";
capture("04-readonly-a", 'bash', '-c', "\n        declare -a MY_ARR=(zero one two)\n        readonly -a MY_ARR\n        echo \"MY_ARR[0]=\${MY_ARR[0]}\"\n        echo \"MY_ARR[1]=\${MY_ARR[1]}\"\n        # Attempt to change an element (will fail)\n        MY_ARR[0]=ZERO 2>/dev/null || echo \"Array assignment denied\"\n        echo \"MY_ARR[0] after attempt=\${MY_ARR[0]}\"\n    ");
print "============================================================\n";
print " SECTION 5: readonly -A  (associative array)\n";
print "============================================================\n";
capture("05-readonly-A", 'bash', '-c', "\n        declare -A MY_ASSOC=([key1]=val1 [key2]=val2)\n        readonly -A MY_ASSOC\n        echo \"MY_ASSOC[key1]=\${MY_ASSOC[key1]}\"\n        echo \"MY_ASSOC[key2]=\${MY_ASSOC[key2]}\"\n        # Attempt to change (will fail)\n        MY_ASSOC[key1]=CHANGED 2>/dev/null || echo \"Assoc assignment denied\"\n        echo \"MY_ASSOC[key1] after attempt=\${MY_ASSOC[key1]}\"\n    ");
print "============================================================\n";
print " SECTION 6: readonly --  (no more options)\n";
print "============================================================\n";
capture("06a-readonly-dash-ok", 'bash', '-c', "\n        readonly -- MYVAR=hello\n        echo \"MYVAR=\${MYVAR}\"\n        # Confirm it is indeed readonly\n        MYVAR=world 2>/dev/null || echo \"Assignment denied (expected)\"\n    ");
capture("06b-readonly-dash-invalid", 'bash', '-c', "\n        readonly -- -notvalid 2>&1 || true\n    ");
print "============================================================\n";
print " SECTION 7: Invalid variable name (error case)\n";
print "============================================================\n";
capture("07-invalid-name", 'bash', '-c', "\n        readonly 123invalid 2>&1 || true\n    ");
print "============================================================\n";
print " SECTION 8: readonly -f -p  (list readonly functions)\n";
print "============================================================\n";
capture("08-readonly-f-p", 'bash', '-c', "\n        f1() { :; }\n        f2() { :; }\n        readonly -f f1 f2\n        readonly -f -p | head -5\n    ");
print "\n";
print "All readonly demo sections completed.\n";


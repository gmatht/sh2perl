#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '016_grep_basic.sh';
my $grep_result_172;
my @grep_lines_172 = ();
my @grep_filenames_172 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_172, $line;
        push @grep_filenames_172, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_172 = grep { /pattern/ } @grep_lines_172;
$grep_result_172 = join "\n", @grep_filtered_172;
if (!($grep_result_172 =~ m{\n\z} || $grep_result_172 eq q{})) {
    $grep_result_172 .= "\n";
}
print $grep_result_172;
$CHILD_ERROR = scalar @grep_filtered_172 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
my $output_173 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'HELLO world' | grep -i hello}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_173, "\n");
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
my $output_174 = do { open(my $__fh, '-|', 'bash', '-c', 'echo -e "line1\\\\nline2\\\\nline3" | grep -v line2') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_174, "\n");
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
my $output_175 = do { open(my $__fh, '-|', 'bash', '-c', 'echo -e "first\\\\nsecond\\\\nthird" | grep -n second') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_175, "\n");
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
my $output_176 = do { open(my $__fh, '-|', 'bash', '-c', 'echo -e "match\\\\nno match\\\\nmatch again" | grep -c match') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_176, "\n");
# Original bash: echo "text with pattern123 in it" | grep -o "pattern[0-9]\+"
my $output_177 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern123 in it' | grep -o "pattern[0-9]\\+"}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_177, "\n");
my $matched = do { my $__cs = do { my $input_data = "test"; my $grep_result_178;
my @grep_lines_178 = split /\n/msx, $input_data;
my @grep_filtered_178 = grep { /.*/s } @grep_lines_178;
$grep_result_178 = scalar @grep_filtered_178 . "\n";
$CHILD_ERROR = scalar @grep_filtered_178 > 0 ? 0 : 1;
 }; chomp $__cs; $__cs; };
print("  grep_exit: " . ($? >> 8), "\n");
print "  match_count: ${matched}\n";

exit $main_exit_code;

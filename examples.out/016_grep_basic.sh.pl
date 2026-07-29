#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;

my $grep_result_0;
my @grep_lines_0 = ();
my @grep_filenames_0 = ();
if (-e "/dev/null") {
    open my $fh, '<', "/dev/null" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_0, $line;
        push @grep_filenames_0, "/dev/null";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /dev/null: No such file or directory\n"; }
my @grep_filtered_0 = grep { /pattern/ } @grep_lines_0;
$grep_result_0 = join "\n", @grep_filtered_0;
if (!($grep_result_0 =~ m{\n\z} || $grep_result_0 eq q{})) {
    $grep_result_0 .= "\n";
}
print $grep_result_0;
$CHILD_ERROR = scalar @grep_filtered_0 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
        print "No matches found\n";
}
# Original bash: echo "HELLO world" | grep -i "hello"
my $output_1 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'HELLO world' | grep -i hello}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_1, "\n";
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
my $output_2 = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "line1\\\\nline2\\\\nline3" | grep -v line2}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_2, "\n";
# Original bash: echo -e "first\nsecond\nthird" | grep -n "second"
my $output_3 = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "first\\\\nsecond\\\\nthird" | grep -n second}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_3, "\n";
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
my $output_4 = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "match\\\\nno match\\\\nmatch again" | grep -c match}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_4, "\n";
# Original bash: echo "text with pattern123 in it" | grep -o "pattern[0-9]\+"
my $output_5 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern123 in it' | grep -o "pattern[0-9]\\\\+"}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_5, "\n";
my $matched = do { my $input_data = "test"; my $grep_result_6;
my @grep_lines_6 = split /\n/msx, $input_data;
my @grep_filtered_6 = grep { /.*/s } @grep_lines_6;
$grep_result_6 = scalar @grep_filtered_6 . "\n";
$CHILD_ERROR = scalar @grep_filtered_6 > 0 ? 0 : 1;
 };
print "  grep_exit: " . ($? >> 8), "\n";
print "  match_count: $matched\n";

exit $main_exit_code;


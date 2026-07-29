#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use File::Path qw(make_path remove_tree);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;

# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
my $output_0 = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "match1\\\\nmatch2\\\\nmatch3\\\\nmatch4" | grep -m 2 match}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_0, "\n";
# Original bash: echo "text with pattern in it" | grep -b "pattern"
my $output_1 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern in it' | grep -b pattern}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_1, "\n";
open my $fh, '>', 'temp_file.txt' or die "temp_file.txt: $!\n";
print {*fh} "content", "\n";
close $fh;
my $grep_result_2;
my @grep_lines_2 = ();
my @grep_filenames_2 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_2, $line;
        push @grep_filenames_2, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_2 = grep { /content/ } @grep_lines_2;
$grep_result_2 = join "\n", @grep_filtered_2;
if (!($grep_result_2 =~ m{\n\z} || $grep_result_2 eq q{})) {
    $grep_result_2 .= "\n";
}
print $grep_result_2;
$CHILD_ERROR = scalar @grep_filtered_2 > 0 ? 0 : 1;
my $grep_result_3;
my @grep_lines_3 = ();
my @grep_filenames_3 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_3, $line;
        push @grep_filenames_3, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_3 = grep { /content/ } @grep_lines_3;
my @grep_with_filename_3;
for my $line (@grep_filtered_3) {
    push @grep_with_filename_3, "temp_file.txt:$line";
}
$grep_result_3 = join "\n", @grep_with_filename_3;
if (!($grep_result_3 =~ m{\n\z} || $grep_result_3 eq q{})) {
    $grep_result_3 .= "\n";
}
print $grep_result_3;
$CHILD_ERROR = scalar @grep_filtered_3 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
my $output_4 = do { open(my $__fh, '-|', 'bash', '-c', q{grep -Z -l pattern temp_file.txt | tr "\\\\0" "\\\\n"}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_4, "\n";
my $output_5 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern in it' | grep --color=always pattern}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_5, "\n";
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_6;
    my @grep_lines_6 = ();
    my @grep_filenames_6 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_6, $line;
            push @grep_filenames_6, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_6 = grep { /pattern/ } @grep_lines_6;
    $grep_result_6 = join "\n", @grep_filtered_6;
        if (!($grep_result_6 =~ m{\n\z} || $grep_result_6 eq q{})) {
            $grep_result_6 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_6 > 0 ? 0 : 1;
    $grep_result_6 = q{};
    $CHILD_ERROR == 0
}) {
        print "found\n";
}
if ($CHILD_ERROR != 0) {
        print "not found\n";
}
if ( -e "temp_file.txt" ) {
    if ( -d "temp_file.txt" ) {
        croak "rm: ", "temp_file.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp_file.txt" ) {
                    }
        else {
            croak "rm: cannot remove ", "temp_file.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "temp_file.txt", ": No such file or directory\n";
}
my $matched = do { my $input_data = "test"; my $grep_result_7;
my @grep_lines_7 = split /\n/msx, $input_data;
my @grep_filtered_7 = grep { /.*/s } @grep_lines_7;
$grep_result_7 = scalar @grep_filtered_7 . "\n";
$CHILD_ERROR = scalar @grep_filtered_7 > 0 ? 0 : 1;
 };
print "  grep_exit: " . ($? >> 8), "\n";
print "  match_count: $matched\n";

exit $main_exit_code;


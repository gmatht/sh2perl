#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '015_grep_advanced.sh';
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
my $output_164 = do { open(my $__fh, '-|', 'bash', '-c', 'echo -e "match1\\\\nmatch2\\\\nmatch3\\\\nmatch4" | grep -m 2 match') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_164, "\n");
# Original bash: echo "text with pattern in it" | grep -b "pattern"
my $output_165 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern in it' | grep -b pattern}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_165, "\n");
open my $fh, '>', 'temp_file.txt' or die "temp_file.txt: $!\n";
print {$fh}("content", "\n");
close $fh;
my $grep_result_166;
my @grep_lines_166 = ();
my @grep_filenames_166 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_166, $line;
        push @grep_filenames_166, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_166 = grep { /content/ } @grep_lines_166;
$grep_result_166 = join "\n", @grep_filtered_166;
if (!($grep_result_166 =~ m{\n\z} || $grep_result_166 eq q{})) {
    $grep_result_166 .= "\n";
}
print $grep_result_166;
$CHILD_ERROR = scalar @grep_filtered_166 > 0 ? 0 : 1;
my $grep_result_167;
my @grep_lines_167 = ();
my @grep_filenames_167 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_167, $line;
        push @grep_filenames_167, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_167 = grep { /content/ } @grep_lines_167;
my @grep_with_filename_167;
for my $line (@grep_filtered_167) {
    push @grep_with_filename_167, "temp_file.txt:$line";
}
$grep_result_167 = join "\n", @grep_with_filename_167;
if (!($grep_result_167 =~ m{\n\z} || $grep_result_167 eq q{})) {
    $grep_result_167 .= "\n";
}
print $grep_result_167;
$CHILD_ERROR = scalar @grep_filtered_167 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
my $output_168 = do { open(my $__fh, '-|', 'bash', '-c', 'grep -Z -l pattern temp_file.txt | tr "\\\\0" "\\\\n"') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_168, "\n");
my $output_169 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern in it' | grep --color=always pattern}) or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_169, "\n");
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_170;
    my @grep_lines_170 = ();
    my @grep_filenames_170 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_170, $line;
            push @grep_filenames_170, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_170 = grep { /pattern/ } @grep_lines_170;
    $grep_result_170 = join "\n", @grep_filtered_170;
        if (!($grep_result_170 =~ m{\n\z} || $grep_result_170 eq q{})) {
            $grep_result_170 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_170 > 0 ? 0 : 1;
    $grep_result_170 = q{};
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
my $matched = do { my $__cs = do { my $input_data = "test"; my $grep_result_171;
my @grep_lines_171 = split /\n/msx, $input_data;
my @grep_filtered_171 = grep { /.*/s } @grep_lines_171;
$grep_result_171 = scalar @grep_filtered_171 . "\n";
$CHILD_ERROR = scalar @grep_filtered_171 > 0 ? 0 : 1;
 }; chomp $__cs; $__cs; };
print("  grep_exit: " . ($? >> 8), "\n");
print "  match_count: ${matched}\n";

exit $main_exit_code;

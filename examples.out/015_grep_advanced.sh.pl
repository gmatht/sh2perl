#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /match/msx } @grep_lines_0_1;
    @grep_filtered_0_1 = @grep_filtered_0_1[0..1];
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
    if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
    $grep_result_0_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    $output_0 = $grep_result_0_1;
    if ((scalar @grep_filtered_0_1) == 0) {
        $pipeline_success_0 = 0;
    }
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_1 = q{};
    my $output_printed_1;
    my $pipeline_success_1 = 1;
    $output_1 .= 'text with pattern in it' . "\n";
if ( !($output_1 =~ m{\n\z}msx) ) { $output_1 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_1_1;
    my @grep_lines_1_1 = split /\n/msx, $output_1;
    my @grep_filtered_1_1 = grep { /pattern/msx } @grep_lines_1_1;
    my @grep_with_offset_1_1;
    my $offset_1_1 = 0;
    for my $line (@grep_lines_1_1) {
    if (grep { $_ eq $line } @grep_filtered_1_1) {
    push @grep_with_offset_1_1, sprintf "%d:%s", $offset_1_1, $line;
    }
    $offset_1_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_1_1 = join "\n", @grep_with_offset_1_1;
    if (!($grep_result_1_1 =~ m{\n\z}msx || $grep_result_1_1 eq q{})) {
    $grep_result_1_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_1_1 > 0 ? 0 : 1;
    $output_1 = $grep_result_1_1;
    $output_1 = $grep_result_1_1;
    if ((scalar @grep_filtered_1_1) == 0) {
        $pipeline_success_1 = 0;
    }
    if ($output_1 ne q{} && !defined $output_printed_1) {
        print $output_1;
        if (!($output_1 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
    }
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp_file.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $grep_result_2;
my @grep_lines_2 = ();
my @grep_filenames_2 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_2, $line;
        push @grep_filenames_2, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_2 = grep { /content/msx } @grep_lines_2;
$grep_result_2 = join "\n", @grep_filtered_2;
if (!($grep_result_2 =~ m{\n\z}msx || $grep_result_2 eq q{})) {
    $grep_result_2 .= "\n";
}
print $grep_result_2;
$CHILD_ERROR = scalar @grep_filtered_2 > 0 ? 0 : 1;
my $grep_result_3;
my @grep_lines_3 = ();
my @grep_filenames_3 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_3, $line;
        push @grep_filenames_3, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_3 = grep { /content/msx } @grep_lines_3;
my @grep_with_filename_3;
for my $line (@grep_filtered_3) {
    push @grep_with_filename_3, "temp_file.txt:$line";
}
$grep_result_3 = join "\n", @grep_with_filename_3;
if (!($grep_result_3 =~ m{\n\z}msx || $grep_result_3 eq q{})) {
    $grep_result_3 .= "\n";
}
print $grep_result_3;
$CHILD_ERROR = scalar @grep_filtered_3 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_4 = q{};
    my $output_printed_4;
    my $pipeline_success_4 = 1;
        my $grep_result_4_0;
    my @grep_lines_4_0 = ();
    my @grep_filenames_4_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_4_0, $line;
    push @grep_filenames_4_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_4_0 = grep { /pattern/msx } @grep_lines_4_0;
    $grep_result_4_0 = @grep_filtered_4_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_4_0 > 0 ? 0 : 1;
    $output_4 = $grep_result_4_0;
    $output_4 = $grep_result_4_0;
    if ((scalar @grep_filtered_4_0) == 0) {
        $pipeline_success_4 = 0;
    }

        my $set1_5 = "\\0";
    my $set2_5 = "\\n";
    my $input_5 = $output_4;
    # Expand character ranges for tr command
    my $expanded_set1_5 = $set1_5;
    my $expanded_set2_5 = $set2_5;
    # Handle a-z range in set1
    if ($expanded_set1_5 =~ /a-z/msx) {
    $expanded_set1_5 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_5 =~ /A-Z/msx) {
    $expanded_set1_5 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_5 =~ /a-z/msx) {
    $expanded_set2_5 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_5 =~ /A-Z/msx) {
    $expanded_set2_5 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_4_1 = q{};
    for my $char ( split //msx, $input_5 ) {
    my $pos_5 = index $expanded_set1_5, $char;
    if ( $pos_5 >= 0 && $pos_5 < length $expanded_set2_5 ) {
    $tr_result_4_1 .= substr $expanded_set2_5, $pos_5, 1;
    } else {
    $tr_result_4_1 .= $char;
    }
    }
    if (!($tr_result_4_1 =~ m{\n\z}msx || $tr_result_4_1 eq q{})) {
    $tr_result_4_1 .= "\n";
    }
    $output_4 = $tr_result_4_1;
    $output_4 = $tr_result_4_1;
    if ($output_4 ne q{} && !defined $output_printed_4) {
        print $output_4;
        if (!($output_4 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_4 ) { $main_exit_code = 1; }
    }
{
    my $output_6 = q{};
    my $output_printed_6;
    my $pipeline_success_6 = 1;
    $output_6 .= 'text with pattern in it' . "\n";
if ( !($output_6 =~ m{\n\z}msx) ) { $output_6 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_6_1;
    my @grep_lines_6_1 = split /\n/msx, $output_6;
    my @grep_filtered_6_1 = grep { /pattern/msx } @grep_lines_6_1;
    my @grep_colored_6_1;
    for my $line (@grep_filtered_6_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_6_1, $colored_line;
    }
    $grep_result_6_1 = join "\n", @grep_colored_6_1;
    if (!($grep_result_6_1 =~ m{\n\z}msx || $grep_result_6_1 eq q{})) {
    $grep_result_6_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_6_1 > 0 ? 0 : 1;
    $output_6 = $grep_result_6_1;
    $output_6 = $grep_result_6_1;
    if ((scalar @grep_filtered_6_1) == 0) {
        $pipeline_success_6 = 0;
    }
    if ($output_6 ne q{} && !defined $output_printed_6) {
        print $output_6;
        if (!($output_6 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_6 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_7;
    my @grep_lines_7 = ();
    my @grep_filenames_7 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_7, $line;
            push @grep_filenames_7, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_7 = grep { /pattern/msx } @grep_lines_7;
    $grep_result_7 = join "\n", @grep_filtered_7;
        if (!($grep_result_7 =~ m{\n\z}msx || $grep_result_7 eq q{})) {
            $grep_result_7 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_7 > 0 ? 0 : 1;
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
            $main_exit_code = 0;
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

exit $main_exit_code;

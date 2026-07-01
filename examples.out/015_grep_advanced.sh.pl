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
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = '015_grep_advanced.sh';
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_169 = q{};
    my $output_printed_169;
    my $pipeline_success_169 = 1;
    $output_169 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_169 =~ m{\n\z}msx) ) { $output_169 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_169_1;
    my @grep_lines_169_1 = split /\n/msx, $output_169;
    my @grep_filtered_169_1 = grep { /match/msx } @grep_lines_169_1;
    @grep_filtered_169_1 = @grep_filtered_169_1[0..1];
    $grep_result_169_1 = join "\n", @grep_filtered_169_1;
    if (!($grep_result_169_1 =~ m{\n\z}msx || $grep_result_169_1 eq q{})) {
    $grep_result_169_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_169_1 > 0 ? 0 : 1;
    $output_169 = $grep_result_169_1;
    $output_169 = $grep_result_169_1;
    if ((scalar @grep_filtered_169_1) == 0) {
        $pipeline_success_169 = 0;
    }
    if ($output_169 ne q{} && !defined $output_printed_169) {
        print $output_169;
        if (!($output_169 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_169 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_170 = q{};
    my $output_printed_170;
    my $pipeline_success_170 = 1;
    $output_170 .= 'text with pattern in it' . "\n";
if ( !($output_170 =~ m{\n\z}msx) ) { $output_170 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_170_1;
    my @grep_lines_170_1 = split /\n/msx, $output_170;
    my @grep_filtered_170_1 = grep { /pattern/msx } @grep_lines_170_1;
    my @grep_with_offset_170_1;
    my $offset_170_1 = 0;
    for my $line (@grep_lines_170_1) {
    if (grep { $_ eq $line } @grep_filtered_170_1) {
    push @grep_with_offset_170_1, sprintf "%d:%s", $offset_170_1, $line;
    }
    $offset_170_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_170_1 = join "\n", @grep_with_offset_170_1;
    if (!($grep_result_170_1 =~ m{\n\z}msx || $grep_result_170_1 eq q{})) {
    $grep_result_170_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_170_1 > 0 ? 0 : 1;
    $output_170 = $grep_result_170_1;
    $output_170 = $grep_result_170_1;
    if ((scalar @grep_filtered_170_1) == 0) {
        $pipeline_success_170 = 0;
    }
    if ($output_170 ne q{} && !defined $output_printed_170) {
        print $output_170;
        if (!($output_170 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_170 ) { $main_exit_code = 1; }
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
my $grep_result_171;
my @grep_lines_171 = ();
my @grep_filenames_171 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_171, $line;
        push @grep_filenames_171, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_171 = grep { /content/msx } @grep_lines_171;
$grep_result_171 = join "\n", @grep_filtered_171;
if (!($grep_result_171 =~ m{\n\z}msx || $grep_result_171 eq q{})) {
    $grep_result_171 .= "\n";
}
print $grep_result_171;
$CHILD_ERROR = scalar @grep_filtered_171 > 0 ? 0 : 1;
my $grep_result_172;
my @grep_lines_172 = ();
my @grep_filenames_172 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_172, $line;
        push @grep_filenames_172, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_172 = grep { /content/msx } @grep_lines_172;
my @grep_with_filename_172;
for my $line (@grep_filtered_172) {
    push @grep_with_filename_172, "temp_file.txt:$line";
}
$grep_result_172 = join "\n", @grep_with_filename_172;
if (!($grep_result_172 =~ m{\n\z}msx || $grep_result_172 eq q{})) {
    $grep_result_172 .= "\n";
}
print $grep_result_172;
$CHILD_ERROR = scalar @grep_filtered_172 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_173 = q{};
    my $output_printed_173;
    my $pipeline_success_173 = 1;
        my $grep_result_173_0;
    my @grep_lines_173_0 = ();
    my @grep_filenames_173_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_173_0, $line;
    push @grep_filenames_173_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_173_0 = grep { /pattern/msx } @grep_lines_173_0;
    $grep_result_173_0 = @grep_filtered_173_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_173_0 > 0 ? 0 : 1;
    $output_173 = $grep_result_173_0;
    $output_173 = $grep_result_173_0;

        my $set1_174 = "\\0";
    my $set2_174 = "\\n";
    my $input_174 = $output_173;
    # Expand character ranges for tr command
    my $expanded_set1_174 = $set1_174;
    my $expanded_set2_174 = $set2_174;
    # Handle a-z range in set1
    if ($expanded_set1_174 =~ /a-z/msx) {
    $expanded_set1_174 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_174 =~ /A-Z/msx) {
    $expanded_set1_174 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_174 =~ /a-z/msx) {
    $expanded_set2_174 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_174 =~ /A-Z/msx) {
    $expanded_set2_174 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_173_1 = q{};
    for my $char ( split //msx, $input_174 ) {
    my $pos_174 = index $expanded_set1_174, $char;
    if ( $pos_174 >= 0 && $pos_174 < length $expanded_set2_174 ) {
    $tr_result_173_1 .= substr $expanded_set2_174, $pos_174, 1;
    } else {
    $tr_result_173_1 .= $char;
    }
    }
    if (!($tr_result_173_1 =~ m{\n\z}msx || $tr_result_173_1 eq q{})) {
    $tr_result_173_1 .= "\n";
    }
    $output_173 = $tr_result_173_1;
    $output_173 = $tr_result_173_1;
    if ($output_173 ne q{} && !defined $output_printed_173) {
        print $output_173;
        if (!($output_173 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_173 ) { $main_exit_code = 1; }
    }
{
    my $output_175 = q{};
    my $output_printed_175;
    my $pipeline_success_175 = 1;
    $output_175 .= 'text with pattern in it' . "\n";
if ( !($output_175 =~ m{\n\z}msx) ) { $output_175 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_175_1;
    my @grep_lines_175_1 = split /\n/msx, $output_175;
    my @grep_filtered_175_1 = grep { /pattern/msx } @grep_lines_175_1;
    my @grep_colored_175_1;
    for my $line (@grep_filtered_175_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_175_1, $colored_line;
    }
    $grep_result_175_1 = join "\n", @grep_colored_175_1;
    if (!($grep_result_175_1 =~ m{\n\z}msx || $grep_result_175_1 eq q{})) {
    $grep_result_175_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_175_1 > 0 ? 0 : 1;
    $output_175 = $grep_result_175_1;
    $output_175 = $grep_result_175_1;
    if ((scalar @grep_filtered_175_1) == 0) {
        $pipeline_success_175 = 0;
    }
    if ($output_175 ne q{} && !defined $output_printed_175) {
        print $output_175;
        if (!($output_175 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_175 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_176;
    my @grep_lines_176 = ();
    my @grep_filenames_176 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_176, $line;
            push @grep_filenames_176, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_176 = grep { /pattern/msx } @grep_lines_176;
    $grep_result_176 = join "\n", @grep_filtered_176;
        if (!($grep_result_176 =~ m{\n\z}msx || $grep_result_176 eq q{})) {
            $grep_result_176 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_176 > 0 ? 0 : 1;
    $grep_result_176 = q{};
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

exit $main_exit_code;

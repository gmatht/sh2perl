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

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Basic grep parameters ==\n";
# Original bash: echo "text with pattern" | grep -i "PATTERN"
{
<<<<<<< HEAD
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'text with pattern' . "\n";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /PATTERN/msxi } @grep_lines_0_1;
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
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_1 = q{};
    my $output_printed_1;
    my $pipeline_success_1 = 1;
    $output_1 .= "line1\nline2\nline3";
if ( !($output_1 =~ m{\n\z}msx) ) { $output_1 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_1_1;
    my @grep_lines_1_1 = split /\n/msx, $output_1;
    my @grep_filtered_1_1 = grep { !/line2/msx } @grep_lines_1_1;
    $grep_result_1_1 = join "\n", @grep_filtered_1_1;
    if (!($grep_result_1_1 =~ m{\n\z}msx || $grep_result_1_1 eq q{})) {
    $grep_result_1_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_1_1 > 0 ? 0 : 1;
    $output_1 = $grep_result_1_1;
    $output_1 = $grep_result_1_1;
    if ((scalar @grep_filtered_1_1) == 0) {
        $pipeline_success_1 = 0;
=======
    my $output_192 = q{};
    my $output_printed_192;
    my $pipeline_success_192 = 1;
    $output_192 .= 'text with pattern' . "\n";
if ( !($output_192 =~ m{\n\z}msx) ) { $output_192 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_192_1;
    my @grep_lines_192_1 = split /\n/msx, $output_192;
    my @grep_filtered_192_1 = grep { /PATTERN/msxi } @grep_lines_192_1;
    $grep_result_192_1 = join "\n", @grep_filtered_192_1;
    if (!($grep_result_192_1 =~ m{\n\z}msx || $grep_result_192_1 eq q{})) {
    $grep_result_192_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_192_1 > 0 ? 0 : 1;
    $output_192 = $grep_result_192_1;
    $output_192 = $grep_result_192_1;
    if ((scalar @grep_filtered_192_1) == 0) {
        $pipeline_success_192 = 0;
    }
    if ($output_192 ne q{} && !defined $output_printed_192) {
        print $output_192;
        if (!($output_192 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_192 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_193 = q{};
    my $output_printed_193;
    my $pipeline_success_193 = 1;
    $output_193 .= "line1\nline2\nline3";
if ( !($output_193 =~ m{\n\z}msx) ) { $output_193 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_193_1;
    my @grep_lines_193_1 = split /\n/msx, $output_193;
    my @grep_filtered_193_1 = grep { !/line2/msx } @grep_lines_193_1;
    $grep_result_193_1 = join "\n", @grep_filtered_193_1;
    if (!($grep_result_193_1 =~ m{\n\z}msx || $grep_result_193_1 eq q{})) {
    $grep_result_193_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_193_1 > 0 ? 0 : 1;
    $output_193 = $grep_result_193_1;
    $output_193 = $grep_result_193_1;
    if ((scalar @grep_filtered_193_1) == 0) {
        $pipeline_success_193 = 0;
    }
    if ($output_193 ne q{} && !defined $output_printed_193) {
        print $output_193;
        if (!($output_193 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_193 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_194 = q{};
    my $output_printed_194;
    my $pipeline_success_194 = 1;
    $output_194 .= "match\nno match\nmatch again";
if ( !($output_194 =~ m{\n\z}msx) ) { $output_194 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_194_1;
    my @grep_lines_194_1 = split /\n/msx, $output_194;
    my @grep_filtered_194_1 = grep { /match/msx } @grep_lines_194_1;
    $grep_result_194_1 = scalar @grep_filtered_194_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_194_1 > 0 ? 0 : 1;
    $output_194 = $grep_result_194_1;
    $output_194 = $grep_result_194_1;
    if ((scalar @grep_filtered_194_1) == 0) {
        $pipeline_success_194 = 0;
    }
    if ($output_194 ne q{} && !defined $output_printed_194) {
        print $output_194;
        if (!($output_194 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_194 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_195 = q{};
    my $output_printed_195;
    my $pipeline_success_195 = 1;
    $output_195 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_195 =~ m{\n\z}msx) ) { $output_195 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_195_1;
    my @grep_lines_195_1 = split /\n/msx, $output_195;
    my @grep_filtered_195_1 = grep { /TARGET/msx } @grep_lines_195_1;
    my @grep_with_context_195_1;
    for my $i (0..@grep_lines_195_1-1) {
    if (scalar grep { $_ eq $grep_lines_195_1[$i] } @grep_filtered_195_1) {
    push @grep_with_context_195_1, $grep_lines_195_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_195_1, $grep_lines_195_1[$j];
    }
    }
    }
    $grep_result_195_1 = join "\n", @grep_with_context_195_1;
    $CHILD_ERROR = scalar @grep_filtered_195_1 > 0 ? 0 : 1;
    $output_195 = $grep_result_195_1;
    $output_195 = $grep_result_195_1;
    if ((scalar @grep_filtered_195_1) == 0) {
        $pipeline_success_195 = 0;
    }
    if ($output_195 ne q{} && !defined $output_printed_195) {
        print $output_195;
        if (!($output_195 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_195 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_196 = q{};
    my $output_printed_196;
    my $pipeline_success_196 = 1;
    $output_196 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_196 =~ m{\n\z}msx) ) { $output_196 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_196_1;
    my @grep_lines_196_1 = split /\n/msx, $output_196;
    my @grep_filtered_196_1 = grep { /TARGET/msx } @grep_lines_196_1;
    my @grep_with_context_196_1;
    for my $i (0..@grep_lines_196_1-1) {
    if (scalar grep { $_ eq $grep_lines_196_1[$i] } @grep_filtered_196_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_196_1, $grep_lines_196_1[$j];
    }
    }
    push @grep_with_context_196_1, $grep_lines_196_1[$i];
    }
    }
    $grep_result_196_1 = join "\n", @grep_with_context_196_1;
    $CHILD_ERROR = scalar @grep_filtered_196_1 > 0 ? 0 : 1;
    $output_196 = $grep_result_196_1;
    $output_196 = $grep_result_196_1;
    if ((scalar @grep_filtered_196_1) == 0) {
        $pipeline_success_196 = 0;
    }
    if ($output_196 ne q{} && !defined $output_printed_196) {
        print $output_196;
        if (!($output_196 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_196 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_197 = q{};
    my $output_printed_197;
    my $pipeline_success_197 = 1;
    $output_197 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_197 =~ m{\n\z}msx) ) { $output_197 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_197_1;
    my @grep_lines_197_1 = split /\n/msx, $output_197;
    my @grep_filtered_197_1 = grep { /TARGET/msx } @grep_lines_197_1;
    my @grep_with_context_197_1;
    for my $i (0..@grep_lines_197_1-1) {
    if (scalar grep { $_ eq $grep_lines_197_1[$i] } @grep_filtered_197_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_197_1, $grep_lines_197_1[$j];
    }
    }
    push @grep_with_context_197_1, $grep_lines_197_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_197_1, $grep_lines_197_1[$j];
    }
    }
    }
    $grep_result_197_1 = join "\n", @grep_with_context_197_1;
    $CHILD_ERROR = scalar @grep_filtered_197_1 > 0 ? 0 : 1;
    $output_197 = $grep_result_197_1;
    $output_197 = $grep_result_197_1;
    if ((scalar @grep_filtered_197_1) == 0) {
        $pipeline_success_197 = 0;
    }
    if ($output_197 ne q{} && !defined $output_printed_197) {
        print $output_197;
        if (!($output_197 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_197 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== File handling parameters ==\n";
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
my $grep_result_198;
my @grep_lines_198 = ();
my @grep_filenames_198 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_198, $line;
        push @grep_filenames_198, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_198 = grep { /content/msx } @grep_lines_198;
my @grep_with_filename_198;
for my $line (@grep_filtered_198) {
    push @grep_with_filename_198, "temp_file.txt:$line";
}
$grep_result_198 = join "\n", @grep_with_filename_198;
if (!($grep_result_198 =~ m{\n\z}msx || $grep_result_198 eq q{})) {
    $grep_result_198 .= "\n";
}
print $grep_result_198;
$CHILD_ERROR = scalar @grep_filtered_198 > 0 ? 0 : 1;
my $grep_result_199;
my @grep_lines_199 = ();
my @grep_filenames_199 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_199, $line;
        push @grep_filenames_199, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_199 = grep { /content/msx } @grep_lines_199;
$grep_result_199 = join "\n", @grep_filtered_199;
if (!($grep_result_199 =~ m{\n\z}msx || $grep_result_199 eq q{})) {
    $grep_result_199 .= "\n";
}
print $grep_result_199;
$CHILD_ERROR = scalar @grep_filtered_199 > 0 ? 0 : 1;
my $grep_result_200;
my @grep_lines_200 = ();
my @grep_filenames_200 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_200, $line;
        push @grep_filenames_200, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_200 = grep { /content/msx } @grep_lines_200;
$grep_result_200 = @grep_filtered_200 > 0 ? "temp_file.txt" : "";
print $grep_result_200;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_200 > 0 ? 0 : 1;
my $grep_result_201;
my @grep_lines_201 = ();
my @grep_filenames_201 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_201, $line;
        push @grep_filenames_201, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_201 = grep { /nonexistent/msx } @grep_lines_201;
$grep_result_201 = @grep_filtered_201 == 0 ? "temp_file.txt" : "";
print $grep_result_201;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_201 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
{
    my $output_203 = q{};
    my $output_printed_203;
    my $pipeline_success_203 = 1;
    $output_203 .= 'text with pattern in it' . "\n";
if ( !($output_203 =~ m{\n\z}msx) ) { $output_203 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_203_1;
    my @grep_lines_203_1 = split /\n/msx, $output_203;
    my @grep_filtered_203_1 = grep { /pattern/msx } @grep_lines_203_1;
    my @grep_matches_203_1;
    foreach my $line (@grep_filtered_203_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_203_1, $1;
    }
    }
    $grep_result_203_1 = join "\n", @grep_matches_203_1;
    $CHILD_ERROR = scalar @grep_filtered_203_1 > 0 ? 0 : 1;
    $output_203 = $grep_result_203_1;
    $output_203 = $grep_result_203_1;
    if ((scalar @grep_filtered_203_1) == 0) {
        $pipeline_success_203 = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    if ($output_1 ne q{} && !defined $output_printed_1) {
        print $output_1;
        if (!($output_1 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
<<<<<<< HEAD
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
=======
    if ( !$pipeline_success_203 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
<<<<<<< HEAD
    my $output_2 = q{};
    my $output_printed_2;
    my $pipeline_success_2 = 1;
    $output_2 .= "match\nno match\nmatch again";
if ( !($output_2 =~ m{\n\z}msx) ) { $output_2 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_2_1;
    my @grep_lines_2_1 = split /\n/msx, $output_2;
    my @grep_filtered_2_1 = grep { /match/msx } @grep_lines_2_1;
    $grep_result_2_1 = scalar @grep_filtered_2_1;
    $CHILD_ERROR = scalar @grep_filtered_2_1 > 0 ? 0 : 1;
    $output_2 = $grep_result_2_1;
    $output_2 = $grep_result_2_1;
    if ((scalar @grep_filtered_2_1) == 0) {
        $pipeline_success_2 = 0;
=======
    my $output_204 = q{};
    my $output_printed_204;
    my $pipeline_success_204 = 1;
    $output_204 .= 'text with pattern in it' . "\n";
if ( !($output_204 =~ m{\n\z}msx) ) { $output_204 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_204_1;
    my @grep_lines_204_1 = split /\n/msx, $output_204;
    my @grep_filtered_204_1 = grep { /pattern/msx } @grep_lines_204_1;
    my @grep_with_offset_204_1;
    my $offset_204_1 = 0;
    for my $line (@grep_lines_204_1) {
    if (grep { $_ eq $line } @grep_filtered_204_1) {
    push @grep_with_offset_204_1, sprintf "%d:%s", $offset_204_1, $line;
    }
    $offset_204_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_204_1 = join "\n", @grep_with_offset_204_1;
    if (!($grep_result_204_1 =~ m{\n\z}msx || $grep_result_204_1 eq q{})) {
    $grep_result_204_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_204_1 > 0 ? 0 : 1;
    $output_204 = $grep_result_204_1;
    $output_204 = $grep_result_204_1;
    if ((scalar @grep_filtered_204_1) == 0) {
        $pipeline_success_204 = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    if ($output_2 ne q{} && !defined $output_printed_2) {
        print $output_2;
        if (!($output_2 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
<<<<<<< HEAD
    if ( !$pipeline_success_2 ) { $main_exit_code = 1; }
=======
    if ( !$pipeline_success_204 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
<<<<<<< HEAD
    my $output_3 = q{};
    my $output_printed_3;
    my $pipeline_success_3 = 1;
    $output_3 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_3 =~ m{\n\z}msx) ) { $output_3 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_3_1;
    my @grep_lines_3_1 = split /\n/msx, $output_3;
    my @grep_filtered_3_1 = grep { /TARGET/msx } @grep_lines_3_1;
    my @grep_with_context_3_1;
    for my $i (0..@grep_lines_3_1-1) {
    if (scalar grep { $_ eq $grep_lines_3_1[$i] } @grep_filtered_3_1) {
    push @grep_with_context_3_1, $grep_lines_3_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_3_1, $grep_lines_3_1[$j];
    }
    }
    }
    $grep_result_3_1 = join "\n", @grep_with_context_3_1;
    $CHILD_ERROR = scalar @grep_filtered_3_1 > 0 ? 0 : 1;
    $output_3 = $grep_result_3_1;
    $output_3 = $grep_result_3_1;
    if ((scalar @grep_filtered_3_1) == 0) {
        $pipeline_success_3 = 0;
=======
    my $output_205 = q{};
    my $output_printed_205;
    my $pipeline_success_205 = 1;
    $output_205 .= 'text with pattern in it' . "\n";
if ( !($output_205 =~ m{\n\z}msx) ) { $output_205 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_205_1;
    my @grep_lines_205_1 = split /\n/msx, $output_205;
    my @grep_filtered_205_1 = grep { /pattern/msx } @grep_lines_205_1;
    my @grep_numbered_205_1;
    for my $i (0..@grep_lines_205_1-1) {
    if (scalar grep { $_ eq $grep_lines_205_1[$i] } @grep_filtered_205_1) {
    push @grep_numbered_205_1, sprintf "%d:%s", $i + 1, $grep_lines_205_1[$i];
    }
    }
    $grep_result_205_1 = join "\n", @grep_numbered_205_1;
    $CHILD_ERROR = scalar @grep_filtered_205_1 > 0 ? 0 : 1;
    $output_205 = $grep_result_205_1;
    $output_205 = $grep_result_205_1;
    if ((scalar @grep_filtered_205_1) == 0) {
        $pipeline_success_205 = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    if ($output_3 ne q{} && !defined $output_printed_3) {
        print $output_3;
        if (!($output_3 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
<<<<<<< HEAD
    if ( !$pipeline_success_3 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_4 = q{};
    my $output_printed_4;
    my $pipeline_success_4 = 1;
    $output_4 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_4 =~ m{\n\z}msx) ) { $output_4 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_4_1;
    my @grep_lines_4_1 = split /\n/msx, $output_4;
    my @grep_filtered_4_1 = grep { /TARGET/msx } @grep_lines_4_1;
    my @grep_with_context_4_1;
    for my $i (0..@grep_lines_4_1-1) {
    if (scalar grep { $_ eq $grep_lines_4_1[$i] } @grep_filtered_4_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_4_1, $grep_lines_4_1[$j];
    }
    }
    push @grep_with_context_4_1, $grep_lines_4_1[$i];
    }
    }
    $grep_result_4_1 = join "\n", @grep_with_context_4_1;
    $CHILD_ERROR = scalar @grep_filtered_4_1 > 0 ? 0 : 1;
    $output_4 = $grep_result_4_1;
    $output_4 = $grep_result_4_1;
    if ((scalar @grep_filtered_4_1) == 0) {
        $pipeline_success_4 = 0;
    }
    if ($output_4 ne q{} && !defined $output_printed_4) {
        print $output_4;
        if (!($output_4 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_4 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_5 = q{};
    my $output_printed_5;
    my $pipeline_success_5 = 1;
    $output_5 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_5 =~ m{\n\z}msx) ) { $output_5 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_5_1;
    my @grep_lines_5_1 = split /\n/msx, $output_5;
    my @grep_filtered_5_1 = grep { /TARGET/msx } @grep_lines_5_1;
    my @grep_with_context_5_1;
    for my $i (0..@grep_lines_5_1-1) {
    if (scalar grep { $_ eq $grep_lines_5_1[$i] } @grep_filtered_5_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_5_1, $grep_lines_5_1[$j];
    }
    }
    push @grep_with_context_5_1, $grep_lines_5_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_5_1, $grep_lines_5_1[$j];
    }
    }
    }
    $grep_result_5_1 = join "\n", @grep_with_context_5_1;
    $CHILD_ERROR = scalar @grep_filtered_5_1 > 0 ? 0 : 1;
    $output_5 = $grep_result_5_1;
    $output_5 = $grep_result_5_1;
    if ((scalar @grep_filtered_5_1) == 0) {
        $pipeline_success_5 = 0;
    }
    if ($output_5 ne q{} && !defined $output_printed_5) {
        print $output_5;
        if (!($output_5 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_5 ) { $main_exit_code = 1; }
    }
print "== File handling parameters ==\n";
=======
    if ( !$pipeline_success_205 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Recursive and include/exclude parameters ==\n";
use File::Path qw(make_path);
my $err;
if ( !-d 'test_dir' ) {
    make_path( 'test_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_dir' . ": $err->[0]\n";
    }
}
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_dir/file1.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "pattern here\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
<<<<<<< HEAD
my $grep_result_6;
my @grep_lines_6 = ();
my @grep_filenames_6 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_6, $line;
        push @grep_filenames_6, "temp_file.txt";
=======
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_dir/file2.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "no pattern\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $grep_result_207;
my @grep_lines_207 = ();
my @grep_filenames_207 = ();
sub find_files_recursive_207 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_207($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
}
my @files_207 = find_files_recursive_207('test_dir', '*');
for my $file (@files_207) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_207, $line;
            push @grep_filenames_207, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_207 = grep { /pattern/msx } @grep_lines_207;
my @grep_with_filename_207;
for my $i (0..@grep_lines_207-1) {
    if (scalar grep { $_ eq $grep_lines_207[$i] } @grep_filtered_207) {
        push @grep_with_filename_207, "$grep_filenames_207[$i]:$grep_lines_207[$i]";
    }
}
$grep_result_207 = join "\n", @grep_with_filename_207;
if (!($grep_result_207 =~ m{\n\z}msx || $grep_result_207 eq q{})) {
    $grep_result_207 .= "\n";
}
print $grep_result_207;
$CHILD_ERROR = scalar @grep_filtered_207 > 0 ? 0 : 1;
my $grep_result_208;
my @grep_lines_208 = ();
my @grep_filenames_208 = ();
sub find_files_recursive_208 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_208($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    return @files;
}
<<<<<<< HEAD
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_6 = grep { /content/msx } @grep_lines_6;
my @grep_with_filename_6;
for my $line (@grep_filtered_6) {
    push @grep_with_filename_6, "temp_file.txt:$line";
=======
my @files_208 = find_files_recursive_208('test_dir', '*.txt');
for my $file (@files_208) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_208, $line;
            push @grep_filenames_208, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_208 = grep { /pattern/msx } @grep_lines_208;
my @grep_with_filename_208;
for my $i (0..@grep_lines_208-1) {
    if (scalar grep { $_ eq $grep_lines_208[$i] } @grep_filtered_208) {
        push @grep_with_filename_208, "$grep_filenames_208[$i]:$grep_lines_208[$i]";
    }
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
}
$grep_result_6 = join "\n", @grep_with_filename_6;
if (!($grep_result_6 =~ m{\n\z}msx || $grep_result_6 eq q{})) {
    $grep_result_6 .= "\n";
}
<<<<<<< HEAD
print $grep_result_6;
$CHILD_ERROR = scalar @grep_filtered_6 > 0 ? 0 : 1;
my $grep_result_7;
my @grep_lines_7 = ();
my @grep_filenames_7 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_7, $line;
        push @grep_filenames_7, "temp_file.txt";
=======
print $grep_result_208;
$CHILD_ERROR = scalar @grep_filtered_208 > 0 ? 0 : 1;
my $grep_result_209;
my @grep_lines_209 = ();
my @grep_filenames_209 = ();
sub find_files_recursive_209 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_209($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx && $file !~ /.*[.]bak$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    return @files;
}
<<<<<<< HEAD
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_7 = grep { /content/msx } @grep_lines_7;
$grep_result_7 = join "\n", @grep_filtered_7;
if (!($grep_result_7 =~ m{\n\z}msx || $grep_result_7 eq q{})) {
    $grep_result_7 .= "\n";
}
print $grep_result_7;
$CHILD_ERROR = scalar @grep_filtered_7 > 0 ? 0 : 1;
my $grep_result_8;
my @grep_lines_8 = ();
my @grep_filenames_8 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_8, $line;
        push @grep_filenames_8, "temp_file.txt";
=======
my @files_209 = find_files_recursive_209('test_dir', '*');
for my $file (@files_209) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_209, $line;
            push @grep_filenames_209, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_209 = grep { /pattern/msx } @grep_lines_209;
my @grep_with_filename_209;
for my $i (0..@grep_lines_209-1) {
    if (scalar grep { $_ eq $grep_lines_209[$i] } @grep_filtered_209) {
        push @grep_with_filename_209, "$grep_filenames_209[$i]:$grep_lines_209[$i]";
    }
}
$grep_result_209 = join "\n", @grep_with_filename_209;
if (!($grep_result_209 =~ m{\n\z}msx || $grep_result_209 eq q{})) {
    $grep_result_209 .= "\n";
}
print $grep_result_209;
$CHILD_ERROR = scalar @grep_filtered_209 > 0 ? 0 : 1;
my $grep_result_210;
my @grep_lines_210 = ();
my @grep_filenames_210 = ();
sub find_files_recursive_210 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_210($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    return @files;
}
<<<<<<< HEAD
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_8 = grep { /content/msx } @grep_lines_8;
$grep_result_8 = @grep_filtered_8 > 0 ? "temp_file.txt" : "";
print $grep_result_8;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_8 > 0 ? 0 : 1;
my $grep_result_9;
my @grep_lines_9 = ();
my @grep_filenames_9 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_9, $line;
        push @grep_filenames_9, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_9 = grep { /nonexistent/msx } @grep_lines_9;
$grep_result_9 = @grep_filtered_9 == 0 ? "temp_file.txt" : "";
print $grep_result_9;
print "\n";
$CHILD_ERROR = $grep_result_9 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
{
    my $output_11 = q{};
    my $output_printed_11;
    my $pipeline_success_11 = 1;
    $output_11 .= 'text with pattern in it' . "\n";
if ( !($output_11 =~ m{\n\z}msx) ) { $output_11 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_11_1;
    my @grep_lines_11_1 = split /\n/msx, $output_11;
    my @grep_filtered_11_1 = grep { /pattern/msx } @grep_lines_11_1;
    my @grep_matches_11_1;
    foreach my $line (@grep_filtered_11_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_11_1, $1;
    }
    }
    $grep_result_11_1 = join "\n", @grep_matches_11_1;
    $CHILD_ERROR = scalar @grep_filtered_11_1 > 0 ? 0 : 1;
    $output_11 = $grep_result_11_1;
    $output_11 = $grep_result_11_1;
    if ((scalar @grep_filtered_11_1) == 0) {
        $pipeline_success_11 = 0;
=======
my @files_210 = find_files_recursive_210('test_dir', '*.txt');
for my $file (@files_210) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_210, $line;
            push @grep_filenames_210, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_210 = grep { /pattern/msx } @grep_lines_210;
my %file_counts_210;
my @file_order_210;
for my $i (0..@grep_lines_210-1) {
    if (scalar grep { $_ eq $grep_lines_210[$i] } @grep_filtered_210) {
        my $f_210 = $grep_filenames_210[$i];
        push @file_order_210, $f_210 unless exists $file_counts_210{$f_210};
        $file_counts_210{$f_210}++;
    }
}
$grep_result_210 = q{};
for my $file (@file_order_210) {
    $grep_result_210 .= "$file:$file_counts_210{$file}\n";
}
$grep_result_210 =~ s/\n$//msx; # Remove trailing newline
print $grep_result_210;
$CHILD_ERROR = scalar @grep_filtered_210 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_211 = q{};
    my $output_printed_211;
    my $pipeline_success_211 = 1;
        my $grep_result_211_0;
    my @grep_lines_211_0 = ();
    my @grep_filenames_211_0 = ();
    sub find_files_recursive_211_0 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, find_files_recursive_211_0($path, $pattern));
    } elsif (-f $path) {
    if ($file =~ /.*[.]txt$/msx) {
    push @files, $path;
    }
    }
    }
    closedir $dh;
    }
    return @files;
    }
    my @files_211_0 = find_files_recursive_211_0('test_dir', '*.txt');
    for my $file (@files_211_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_211_0, $line;
    push @grep_filenames_211_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_211_0 = grep { /pattern/msx } @grep_lines_211_0;
    my @grep_with_filename_211_0;
    for my $i (0..@grep_lines_211_0-1) {
    if (scalar grep { $_ eq $grep_lines_211_0[$i] } @grep_filtered_211_0) {
    push @grep_with_filename_211_0, "$grep_filenames_211_0[$i]:$grep_lines_211_0[$i]";
    }
    }
    $grep_result_211_0 = join "\n", @grep_with_filename_211_0;
    if (!($grep_result_211_0 =~ m{\n\z}msx || $grep_result_211_0 eq q{})) {
    $grep_result_211_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_211_0 > 0 ? 0 : 1;
    $output_211 = $grep_result_211_0;
    $output_211 = $grep_result_211_0;
    if ((scalar @grep_filtered_211_0) == 0) {
        $pipeline_success_211 = 0;
    }

        use IPC::Open3;
    my @wc_args_211_1 = ('-l');
    my ($wc_in_211_1, $wc_out_211_1, $wc_err_211_1);
    my $wc_pid_211_1 = open3($wc_in_211_1, $wc_out_211_1, $wc_err_211_1, 'wc', @wc_args_211_1);
    print {$wc_in_211_1} $output_211;
    close $wc_in_211_1 or die "Close failed: $OS_ERROR\n";
    my $output_211_1 = do { local $/ = undef; <$wc_out_211_1> };
    if ($output_211_1 eq q{}) { $output_211_1 = "0\n"; }
    close $wc_out_211_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_211_1, 0;
    $output_211 = $output_211_1;
    if ($output_211 ne q{} && !defined $output_printed_211) {
        print $output_211;
        if (!($output_211 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_211 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_212 = q{};
    my $output_printed_212;
    my $pipeline_success_212 = 1;
    $output_212 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_212 =~ m{\n\z}msx) ) { $output_212 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_212_1;
    my @grep_lines_212_1 = split /\n/msx, $output_212;
    my @grep_filtered_212_1 = grep { /match/msx } @grep_lines_212_1;
    @grep_filtered_212_1 = @grep_filtered_212_1[0..1];
    $grep_result_212_1 = join "\n", @grep_filtered_212_1;
    if (!($grep_result_212_1 =~ m{\n\z}msx || $grep_result_212_1 eq q{})) {
    $grep_result_212_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_212_1 > 0 ? 0 : 1;
    $output_212 = $grep_result_212_1;
    $output_212 = $grep_result_212_1;
    if ((scalar @grep_filtered_212_1) == 0) {
        $pipeline_success_212 = 0;
    }
    if ($output_212 ne q{} && !defined $output_printed_212) {
        print $output_212;
        if (!($output_212 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_212 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if (do {
{
    my $output_213 = q{};
    my $output_printed_213;
    my $pipeline_success_213 = 1;
    $output_213 .= 'text with pattern in it' . "\n";
if ( !($output_213 =~ m{\n\z}msx) ) { $output_213 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_213_1;
    my @grep_lines_213_1 = split /\n/msx, $output_213;
    my @grep_filtered_213_1 = grep { /pattern/msx } @grep_lines_213_1;
    $grep_result_213_1 = join "\n", @grep_filtered_213_1;
    if (!($grep_result_213_1 =~ m{\n\z}msx || $grep_result_213_1 eq q{})) {
    $grep_result_213_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_213_1 > 0 ? 0 : 1;
    $output_213 = q{};
    if ((scalar @grep_filtered_213_1) == 0) {
        $pipeline_success_213 = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    if ($output_11 ne q{} && !defined $output_printed_11) {
        print $output_11;
        if (!($output_11 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_11 ) { $main_exit_code = 1; }
    }
    $CHILD_ERROR == 0
}) {
        print "found\n";
}
if ($CHILD_ERROR != 0) {
        print "not found\n";
}
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
<<<<<<< HEAD
    my $output_12 = q{};
    my $output_printed_12;
    my $pipeline_success_12 = 1;
    $output_12 .= 'text with pattern in it' . "\n";
if ( !($output_12 =~ m{\n\z}msx) ) { $output_12 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_12_1;
    my @grep_lines_12_1 = split /\n/msx, $output_12;
    my @grep_filtered_12_1 = grep { /pattern/msx } @grep_lines_12_1;
    my @grep_with_offset_12_1;
    my $offset_12_1 = 0;
    for my $line (@grep_lines_12_1) {
    if (grep { $_ eq $line } @grep_filtered_12_1) {
    push @grep_with_offset_12_1, sprintf "%d:%s", $offset_12_1, $line;
    }
    $offset_12_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_12_1 = join "\n", @grep_with_offset_12_1;
    if (!($grep_result_12_1 =~ m{\n\z}msx || $grep_result_12_1 eq q{})) {
    $grep_result_12_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_12_1 > 0 ? 0 : 1;
    $output_12 = $grep_result_12_1;
    $output_12 = $grep_result_12_1;
    if ((scalar @grep_filtered_12_1) == 0) {
        $pipeline_success_12 = 0;
    }
    if ($output_12 ne q{} && !defined $output_printed_12) {
        print $output_12;
        if (!($output_12 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_12 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_13 = q{};
    my $output_printed_13;
    my $pipeline_success_13 = 1;
    $output_13 .= 'text with pattern in it' . "\n";
if ( !($output_13 =~ m{\n\z}msx) ) { $output_13 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_13_1;
    my @grep_lines_13_1 = split /\n/msx, $output_13;
    my @grep_filtered_13_1 = grep { /pattern/msx } @grep_lines_13_1;
    my @grep_numbered_13_1;
    for my $i (0..@grep_lines_13_1-1) {
    if (scalar grep { $_ eq $grep_lines_13_1[$i] } @grep_filtered_13_1) {
    push @grep_numbered_13_1, sprintf "%d:%s", $i + 1, $grep_lines_13_1[$i];
    }
    }
    $grep_result_13_1 = join "\n", @grep_numbered_13_1;
    $CHILD_ERROR = scalar @grep_filtered_13_1 > 0 ? 0 : 1;
    $output_13 = $grep_result_13_1;
    $output_13 = $grep_result_13_1;
    if ((scalar @grep_filtered_13_1) == 0) {
        $pipeline_success_13 = 0;
    }
    if ($output_13 ne q{} && !defined $output_printed_13) {
        print $output_13;
        if (!($output_13 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_13 ) { $main_exit_code = 1; }
    }
print "== Recursive and include/exclude parameters ==\n";
use File::Path qw(make_path);
my $err;
if ( !-d 'test_dir' ) {
    make_path( 'test_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_dir' . ": $err->[0]\n";
    }
}
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'test_dir/file1.txt'
      or die "Cannot open file: $!\n";
    print "pattern here\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'test_dir/file2.txt'
      or die "Cannot open file: $!\n";
    print "no pattern\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
my $grep_result_15;
my @grep_lines_15 = ();
my @grep_filenames_15 = ();
my $find_files_recursive_15;
$find_files_recursive_15 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_15->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_15 = $find_files_recursive_15->('test_dir', '*');
for my $file (@files_15) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_15, $line;
            push @grep_filenames_15, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_15 = grep { /pattern/msx } @grep_lines_15;
my @grep_with_filename_15;
for my $i (0..@grep_lines_15-1) {
    if (scalar grep { $_ eq $grep_lines_15[$i] } @grep_filtered_15) {
        push @grep_with_filename_15, $grep_filenames_15[$i] . ':' . $grep_lines_15[$i];
    }
}
$grep_result_15 = join "\n", @grep_with_filename_15;
if (!($grep_result_15 =~ m{\n\z}msx || $grep_result_15 eq q{})) {
    $grep_result_15 .= "\n";
}
print $grep_result_15;
$CHILD_ERROR = scalar @grep_filtered_15 > 0 ? 0 : 1;
my $grep_result_16;
my @grep_lines_16 = ();
my @grep_filenames_16 = ();
my $find_files_recursive_16;
$find_files_recursive_16 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_16->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_16 = $find_files_recursive_16->('test_dir', '*.txt');
for my $file (@files_16) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_16, $line;
            push @grep_filenames_16, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_16 = grep { /pattern/msx } @grep_lines_16;
my @grep_with_filename_16;
for my $i (0..@grep_lines_16-1) {
    if (scalar grep { $_ eq $grep_lines_16[$i] } @grep_filtered_16) {
        push @grep_with_filename_16, $grep_filenames_16[$i] . ':' . $grep_lines_16[$i];
    }
}
$grep_result_16 = join "\n", @grep_with_filename_16;
if (!($grep_result_16 =~ m{\n\z}msx || $grep_result_16 eq q{})) {
    $grep_result_16 .= "\n";
}
print $grep_result_16;
$CHILD_ERROR = scalar @grep_filtered_16 > 0 ? 0 : 1;
my $grep_result_17;
my @grep_lines_17 = ();
my @grep_filenames_17 = ();
my $find_files_recursive_17;
$find_files_recursive_17 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_17->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx && $file !~ /.*[.]bak$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_17 = $find_files_recursive_17->('test_dir', '*');
for my $file (@files_17) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_17, $line;
            push @grep_filenames_17, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_17 = grep { /pattern/msx } @grep_lines_17;
my @grep_with_filename_17;
for my $i (0..@grep_lines_17-1) {
    if (scalar grep { $_ eq $grep_lines_17[$i] } @grep_filtered_17) {
        push @grep_with_filename_17, $grep_filenames_17[$i] . ':' . $grep_lines_17[$i];
    }
}
$grep_result_17 = join "\n", @grep_with_filename_17;
if (!($grep_result_17 =~ m{\n\z}msx || $grep_result_17 eq q{})) {
    $grep_result_17 .= "\n";
}
print $grep_result_17;
$CHILD_ERROR = scalar @grep_filtered_17 > 0 ? 0 : 1;
my $grep_result_18;
my @grep_lines_18 = ();
my @grep_filenames_18 = ();
my $find_files_recursive_18;
$find_files_recursive_18 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_18->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_18 = $find_files_recursive_18->('test_dir', '*.txt');
for my $file (@files_18) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_18, $line;
            push @grep_filenames_18, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_18 = grep { /pattern/msx } @grep_lines_18;
my %file_counts_18;
for my $i (0..@grep_lines_18-1) {
    if (scalar grep { $_ eq $grep_lines_18[$i] } @grep_filtered_18) {
        $file_counts_18{$grep_filenames_18[$i]}++;
    }
}
$grep_result_18 = q{};
for my $file (sort keys %file_counts_18) {
    $grep_result_18 .= "$file:$file_counts_18{$file}\n";
}
chomp $grep_result_18;
print $grep_result_18;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_18 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_19 = q{};
    my $output_printed_19;
    my $pipeline_success_19 = 1;
        my $grep_result_19_0;
    my @grep_lines_19_0 = ();
    my @grep_filenames_19_0 = ();
    my $find_files_recursive_19_0;
    $find_files_recursive_19_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_19_0->($path, $pattern));
    } elsif (-f $path) {
    if ($file =~ /.*[.]txt$/msx) {
    push @files, $path;
    }
    }
    }
    closedir $dh;
    }
    return @files;
    };
    my @files_19_0 = $find_files_recursive_19_0->('test_dir', '*.txt');
    for my $file (@files_19_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_19_0, $line;
    push @grep_filenames_19_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_19_0 = grep { /pattern/msx } @grep_lines_19_0;
    my @grep_with_filename_19_0;
    for my $i (0..@grep_lines_19_0-1) {
    if (scalar grep { $_ eq $grep_lines_19_0[$i] } @grep_filtered_19_0) {
    push @grep_with_filename_19_0, $grep_filenames_19_0[$i] . ':' . $grep_lines_19_0[$i];
    }
    }
    $grep_result_19_0 = join "\n", @grep_with_filename_19_0;
    if (!($grep_result_19_0 =~ m{\n\z}msx || $grep_result_19_0 eq q{})) {
    $grep_result_19_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_19_0 > 0 ? 0 : 1;
    $output_19 = $grep_result_19_0;
    $output_19 = $grep_result_19_0;
    if ((scalar @grep_filtered_19_0) == 0) {
        $pipeline_success_19 = 0;
    }

        use IPC::Open3;
    my @wc_args_19_1 = ('-l');
    my ($wc_in_19_1, $wc_out_19_1, $wc_err_19_1);
    my $wc_pid_19_1 = open3($wc_in_19_1, $wc_out_19_1, $wc_err_19_1, 'wc', @wc_args_19_1);
    print {$wc_in_19_1} $output_19;
    close $wc_in_19_1 or die "Close failed: $!\n";
    my $output_19_1 = do { local $/ = undef; <$wc_out_19_1> };
    if ($output_19_1 eq q{}) { $output_19_1 = "0\n"; }
    close $wc_out_19_1 or die "Close failed: $!\n";
    waitpid $wc_pid_19_1, 0;
    $output_19 = $output_19_1;
    if ($output_19 ne q{} && !defined $output_printed_19) {
        print $output_19;
        if (!($output_19 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_19 ) { $main_exit_code = 1; }
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_20 = q{};
    my $output_printed_20;
    my $pipeline_success_20 = 1;
    $output_20 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_20 =~ m{\n\z}msx) ) { $output_20 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_20_1;
    my @grep_lines_20_1 = split /\n/msx, $output_20;
    my @grep_filtered_20_1 = grep { /match/msx } @grep_lines_20_1;
    @grep_filtered_20_1 = @grep_filtered_20_1[0..1];
    $grep_result_20_1 = join "\n", @grep_filtered_20_1;
    if (!($grep_result_20_1 =~ m{\n\z}msx || $grep_result_20_1 eq q{})) {
    $grep_result_20_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_20_1 > 0 ? 0 : 1;
    $output_20 = $grep_result_20_1;
    $output_20 = $grep_result_20_1;
    if ((scalar @grep_filtered_20_1) == 0) {
        $pipeline_success_20 = 0;
    }
    if ($output_20 ne q{} && !defined $output_printed_20) {
        print $output_20;
        if (!($output_20 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_20 ) { $main_exit_code = 1; }
    }
if (do {
# Original bash: echo "text with pattern in it" | grep -q "pattern" && echo
{
    my $output_21 = q{};
    my $output_printed_21;
    my $pipeline_success_21 = 1;
    $output_21 .= 'text with pattern in it' . "\n";
if ( !($output_21 =~ m{\n\z}msx) ) { $output_21 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_21_1;
    my @grep_lines_21_1 = split /\n/msx, $output_21;
    my @grep_filtered_21_1 = grep { /pattern/msx } @grep_lines_21_1;
    $grep_result_21_1 = join "\n", @grep_filtered_21_1;
    if (!($grep_result_21_1 =~ m{\n\z}msx || $grep_result_21_1 eq q{})) {
    $grep_result_21_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_21_1 > 0 ? 0 : 1;
    $grep_result_21_1 = q{};
    $output_21 = $grep_result_21_1;
    $output_21 = $grep_result_21_1;
    if ((scalar @grep_filtered_21_1) == 0) {
        $pipeline_success_21 = 0;
    }
    if ($output_21 ne q{} && !defined $output_printed_21) {
        print $output_21;
        if (!($output_21 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_21 ) { $main_exit_code = 1; }
    }
    $CHILD_ERROR == 0
}) {
        print "found\n";
}
if ($CHILD_ERROR != 0) {
        print "not found\n";
}
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_22 = q{};
    my $output_printed_22;
    my $pipeline_success_22 = 1;
        my $grep_result_22_0;
    my @grep_lines_22_0 = ();
    my @grep_filenames_22_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_22_0, $line;
    push @grep_filenames_22_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_22_0 = grep { /pattern/msx } @grep_lines_22_0;
    $grep_result_22_0 = @grep_filtered_22_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_22_0 > 0 ? 0 : 1;
    $output_22 = $grep_result_22_0;
    $output_22 = $grep_result_22_0;
    if ((scalar @grep_filtered_22_0) == 0) {
        $pipeline_success_22 = 0;
    }

        my $set1_23 = "\\0";
    my $set2_23 = "\\n";
    my $input_23 = $output_22;
    # Expand character ranges for tr command
    my $expanded_set1_23 = $set1_23;
    my $expanded_set2_23 = $set2_23;
    # Handle a-z range in set1
    if ($expanded_set1_23 =~ /a-z/msx) {
    $expanded_set1_23 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_23 =~ /A-Z/msx) {
    $expanded_set1_23 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_23 =~ /a-z/msx) {
    $expanded_set2_23 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_23 =~ /A-Z/msx) {
    $expanded_set2_23 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_22_1 = q{};
    for my $char ( split //msx, $input_23 ) {
    my $pos_23 = index $expanded_set1_23, $char;
    if ( $pos_23 >= 0 && $pos_23 < length $expanded_set2_23 ) {
    $tr_result_22_1 .= substr $expanded_set2_23, $pos_23, 1;
    } else {
    $tr_result_22_1 .= $char;
    }
    }
    if (!($tr_result_22_1 =~ m{\n\z}msx || $tr_result_22_1 eq q{})) {
    $tr_result_22_1 .= "\n";
    }
    $output_22 = $tr_result_22_1;
    $output_22 = $tr_result_22_1;
    if ($output_22 ne q{} && !defined $output_printed_22) {
        print $output_22;
        if (!($output_22 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_22 ) { $main_exit_code = 1; }
=======
    my $output_214 = q{};
    my $output_printed_214;
    my $pipeline_success_214 = 1;
        my $grep_result_214_0;
    my @grep_lines_214_0 = ();
    my @grep_filenames_214_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_214_0, $line;
    push @grep_filenames_214_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_214_0 = grep { /pattern/msx } @grep_lines_214_0;
    $grep_result_214_0 = @grep_filtered_214_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_214_0 > 0 ? 0 : 1;
    $output_214 = $grep_result_214_0;
    $output_214 = $grep_result_214_0;
    if ((scalar @grep_filtered_214_0) == 0) {
        $pipeline_success_214 = 0;
    }

        my $set1_215 = "\\0";
    my $set2_215 = "\\n";
    my $input_215 = $output_214;
    # Expand character ranges for tr command
    my $expanded_set1_215 = $set1_215;
    my $expanded_set2_215 = $set2_215;
    # Handle a-z range in set1
    if ($expanded_set1_215 =~ /a-z/msx) {
    $expanded_set1_215 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_215 =~ /A-Z/msx) {
    $expanded_set1_215 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_215 =~ /a-z/msx) {
    $expanded_set2_215 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_215 =~ /A-Z/msx) {
    $expanded_set2_215 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_214_1 = q{};
    for my $char ( split //msx, $input_215 ) {
    my $pos_215 = index $expanded_set1_215, $char;
    if ( $pos_215 >= 0 && $pos_215 < length $expanded_set2_215 ) {
    $tr_result_214_1 .= substr $expanded_set2_215, $pos_215, 1;
    } else {
    $tr_result_214_1 .= $char;
    }
    }
    if (!($tr_result_214_1 =~ m{\n\z}msx || $tr_result_214_1 eq q{})) {
    $tr_result_214_1 .= "\n";
    }
    $output_214 = $tr_result_214_1;
    $output_214 = $tr_result_214_1;
    if ($output_214 ne q{} && !defined $output_printed_214) {
        print $output_214;
        if (!($output_214 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_214 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
if ( -e "temp_file.txt" ) {
    if ( -d "temp_file.txt" ) {
        carp "rm: carping: ", "temp_file.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp_file.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "temp_file.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "test_dir" ) {
    if ( -d "test_dir" ) {
        my $err;
        require File::Path;
        File::Path::remove_tree("test_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_dir", ": $err->[0]\n";
        }
        else {
                    }
    }
    else {
        if ( unlink "test_dir" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "test_dir",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

exit $main_exit_code;

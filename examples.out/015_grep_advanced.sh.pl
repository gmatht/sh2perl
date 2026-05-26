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
    my $output_175 = q{};
    my $output_printed_175;
    my $pipeline_success_175 = 1;
    $output_175 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_175 =~ m{\n\z}msx) ) { $output_175 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_175_1;
    my @grep_lines_175_1 = split /\n/msx, $output_175;
    my @grep_filtered_175_1 = grep { /match/msx } @grep_lines_175_1;
    @grep_filtered_175_1 = @grep_filtered_175_1[0..1];
    $grep_result_175_1 = join "\n", @grep_filtered_175_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_176 = q{};
    my $output_printed_176;
    my $pipeline_success_176 = 1;
    $output_176 .= 'text with pattern in it' . "\n";
if ( !($output_176 =~ m{\n\z}msx) ) { $output_176 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_176_1;
    my @grep_lines_176_1 = split /\n/msx, $output_176;
    my @grep_filtered_176_1 = grep { /pattern/msx } @grep_lines_176_1;
    my @grep_with_offset_176_1;
    my $offset_176_1 = 0;
    for my $line (@grep_lines_176_1) {
    if (grep { $_ eq $line } @grep_filtered_176_1) {
    push @grep_with_offset_176_1, sprintf "%d:%s", $offset_176_1, $line;
    }
    $offset_176_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_176_1 = join "\n", @grep_with_offset_176_1;
    if (!($grep_result_176_1 =~ m{\n\z}msx || $grep_result_176_1 eq q{})) {
    $grep_result_176_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_176_1 > 0 ? 0 : 1;
    $output_176 = $grep_result_176_1;
    $output_176 = $grep_result_176_1;
    if ((scalar @grep_filtered_176_1) == 0) {
        $pipeline_success_176 = 0;
    }
    if ($output_176 ne q{} && !defined $output_printed_176) {
        print $output_176;
        if (!($output_176 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_176 ) { $main_exit_code = 1; }
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
my $grep_result_177;
my @grep_lines_177 = ();
my @grep_filenames_177 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_177, $line;
        push @grep_filenames_177, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_177 = grep { /content/msx } @grep_lines_177;
$grep_result_177 = join "\n", @grep_filtered_177;
if (!($grep_result_177 =~ m{\n\z}msx || $grep_result_177 eq q{})) {
    $grep_result_177 .= "\n";
}
print $grep_result_177;
$CHILD_ERROR = scalar @grep_filtered_177 > 0 ? 0 : 1;
my $grep_result_178;
my @grep_lines_178 = ();
my @grep_filenames_178 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_178, $line;
        push @grep_filenames_178, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_178 = grep { /content/msx } @grep_lines_178;
my @grep_with_filename_178;
for my $line (@grep_filtered_178) {
    push @grep_with_filename_178, "temp_file.txt:$line";
}
$grep_result_178 = join "\n", @grep_with_filename_178;
if (!($grep_result_178 =~ m{\n\z}msx || $grep_result_178 eq q{})) {
    $grep_result_178 .= "\n";
}
print $grep_result_178;
$CHILD_ERROR = scalar @grep_filtered_178 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_179 = q{};
    my $output_printed_179;
    my $pipeline_success_179 = 1;
        my $grep_result_179_0;
    my @grep_lines_179_0 = ();
    my @grep_filenames_179_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_179_0, $line;
    push @grep_filenames_179_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_179_0 = grep { /pattern/msx } @grep_lines_179_0;
    $grep_result_179_0 = @grep_filtered_179_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_179_0 > 0 ? 0 : 1;
    $output_179 = $grep_result_179_0;
    $output_179 = $grep_result_179_0;
    if ((scalar @grep_filtered_179_0) == 0) {
        $pipeline_success_179 = 0;
    }

        my $set1_180 = "\\0";
    my $set2_180 = "\\n";
    my $input_180 = $output_179;
    # Expand character ranges for tr command
    my $expanded_set1_180 = $set1_180;
    my $expanded_set2_180 = $set2_180;
    # Handle a-z range in set1
    if ($expanded_set1_180 =~ /a-z/msx) {
    $expanded_set1_180 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_180 =~ /A-Z/msx) {
    $expanded_set1_180 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_180 =~ /a-z/msx) {
    $expanded_set2_180 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_180 =~ /A-Z/msx) {
    $expanded_set2_180 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_179_1 = q{};
    for my $char ( split //msx, $input_180 ) {
    my $pos_180 = index $expanded_set1_180, $char;
    if ( $pos_180 >= 0 && $pos_180 < length $expanded_set2_180 ) {
    $tr_result_179_1 .= substr $expanded_set2_180, $pos_180, 1;
    } else {
    $tr_result_179_1 .= $char;
    }
    }
    if (!($tr_result_179_1 =~ m{\n\z}msx || $tr_result_179_1 eq q{})) {
    $tr_result_179_1 .= "\n";
    }
    $output_179 = $tr_result_179_1;
    $output_179 = $tr_result_179_1;
    if ($output_179 ne q{} && !defined $output_printed_179) {
        print $output_179;
        if (!($output_179 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_179 ) { $main_exit_code = 1; }
    }
{
    my $output_181 = q{};
    my $output_printed_181;
    my $pipeline_success_181 = 1;
    $output_181 .= 'text with pattern in it' . "\n";
if ( !($output_181 =~ m{\n\z}msx) ) { $output_181 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_181_1;
    my @grep_lines_181_1 = split /\n/msx, $output_181;
    my @grep_filtered_181_1 = grep { /pattern/msx } @grep_lines_181_1;
    my @grep_colored_181_1;
    for my $line (@grep_filtered_181_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_181_1, $colored_line;
    }
    $grep_result_181_1 = join "\n", @grep_colored_181_1;
    if (!($grep_result_181_1 =~ m{\n\z}msx || $grep_result_181_1 eq q{})) {
    $grep_result_181_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_181_1 > 0 ? 0 : 1;
    $output_181 = $grep_result_181_1;
    $output_181 = $grep_result_181_1;
    if ((scalar @grep_filtered_181_1) == 0) {
        $pipeline_success_181 = 0;
    }
    if ($output_181 ne q{} && !defined $output_printed_181) {
        print $output_181;
        if (!($output_181 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_181 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_182;
    my @grep_lines_182 = ();
    my @grep_filenames_182 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_182, $line;
            push @grep_filenames_182, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_182 = grep { /pattern/msx } @grep_lines_182;
    $grep_result_182 = join "\n", @grep_filtered_182;
        if (!($grep_result_182 =~ m{\n\z}msx || $grep_result_182 eq q{})) {
            $grep_result_182 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_182 > 0 ? 0 : 1;
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

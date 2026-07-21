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
    my $output_171 = q{};
    my $output_printed_171;
    my $pipeline_success_171 = 1;
    $output_171 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_171 =~ m{\n\z}msx) ) { $output_171 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_171_1;
    my @grep_lines_171_1 = split /\n/msx, $output_171;
    my @grep_filtered_171_1 = grep { /match/msx } @grep_lines_171_1;
    @grep_filtered_171_1 = @grep_filtered_171_1[0..1];
    $grep_result_171_1 = join "\n", @grep_filtered_171_1;
    if (!($grep_result_171_1 =~ m{\n\z}msx || $grep_result_171_1 eq q{})) {
    $grep_result_171_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_171_1 > 0 ? 0 : 1;
    $output_171 = $grep_result_171_1;
    $output_171 = $grep_result_171_1;
    if ((scalar @grep_filtered_171_1) == 0) {
        $pipeline_success_171 = 0;
    }
    if ($output_171 ne q{} && !defined $output_printed_171) {
        print $output_171;
        if (!($output_171 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_171 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_172 = q{};
    my $output_printed_172;
    my $pipeline_success_172 = 1;
    $output_172 .= 'text with pattern in it' . "\n";
if ( !($output_172 =~ m{\n\z}msx) ) { $output_172 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_172_1;
    my @grep_lines_172_1 = split /\n/msx, $output_172;
    my @grep_filtered_172_1 = grep { /pattern/msx } @grep_lines_172_1;
    my @grep_with_offset_172_1;
    my $offset_172_1 = 0;
    for my $line (@grep_lines_172_1) {
    if (grep { $_ eq $line } @grep_filtered_172_1) {
    push @grep_with_offset_172_1, sprintf "%d:%s", $offset_172_1, $line;
    }
    $offset_172_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_172_1 = join "\n", @grep_with_offset_172_1;
    if (!($grep_result_172_1 =~ m{\n\z}msx || $grep_result_172_1 eq q{})) {
    $grep_result_172_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_172_1 > 0 ? 0 : 1;
    $output_172 = $grep_result_172_1;
    $output_172 = $grep_result_172_1;
    if ((scalar @grep_filtered_172_1) == 0) {
        $pipeline_success_172 = 0;
    }
    if ($output_172 ne q{} && !defined $output_printed_172) {
        print $output_172;
        if (!($output_172 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_172 ) { $main_exit_code = 1; }
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
my $grep_result_173;
my @grep_lines_173 = ();
my @grep_filenames_173 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_173, $line;
        push @grep_filenames_173, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_173 = grep { /content/msx } @grep_lines_173;
$grep_result_173 = join "\n", @grep_filtered_173;
if (!($grep_result_173 =~ m{\n\z}msx || $grep_result_173 eq q{})) {
    $grep_result_173 .= "\n";
}
print $grep_result_173;
$CHILD_ERROR = scalar @grep_filtered_173 > 0 ? 0 : 1;
my $grep_result_174;
my @grep_lines_174 = ();
my @grep_filenames_174 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_174, $line;
        push @grep_filenames_174, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_174 = grep { /content/msx } @grep_lines_174;
my @grep_with_filename_174;
for my $line (@grep_filtered_174) {
    push @grep_with_filename_174, "temp_file.txt:$line";
}
$grep_result_174 = join "\n", @grep_with_filename_174;
if (!($grep_result_174 =~ m{\n\z}msx || $grep_result_174 eq q{})) {
    $grep_result_174 .= "\n";
}
print $grep_result_174;
$CHILD_ERROR = scalar @grep_filtered_174 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_175 = q{};
    my $output_printed_175;
    my $pipeline_success_175 = 1;
        my $grep_result_175_0;
    my @grep_lines_175_0 = ();
    my @grep_filenames_175_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_175_0, $line;
    push @grep_filenames_175_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_175_0 = grep { /pattern/msx } @grep_lines_175_0;
    $grep_result_175_0 = @grep_filtered_175_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_175_0 > 0 ? 0 : 1;
    $output_175 = $grep_result_175_0;
    $output_175 = $grep_result_175_0;

        my $set1_176 = "\\0";
    my $set2_176 = "\\n";
    my $input_176 = $output_175;
    # Expand character ranges for tr command
    my $expanded_set1_176 = $set1_176;
    my $expanded_set2_176 = $set2_176;
    # Handle a-z range in set1
    if ($expanded_set1_176 =~ /a-z/msx) {
    $expanded_set1_176 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_176 =~ /A-Z/msx) {
    $expanded_set1_176 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_176 =~ /\[:upper:\]/msx) {
    $expanded_set1_176 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_176 =~ /\[:lower:\]/msx) {
    $expanded_set1_176 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_176 =~ /a-z/msx) {
    $expanded_set2_176 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_176 =~ /A-Z/msx) {
    $expanded_set2_176 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_176 =~ /\[:upper:\]/msx) {
    $expanded_set2_176 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_176 =~ /\[:lower:\]/msx) {
    $expanded_set2_176 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_175_1 = q{};
    for my $char ( split //msx, $input_176 ) {
    my $pos_176 = index $expanded_set1_176, $char;
    if ( $pos_176 >= 0 && $pos_176 < length $expanded_set2_176 ) {
    $tr_result_175_1 .= substr $expanded_set2_176, $pos_176, 1;
    } else {
    $tr_result_175_1 .= $char;
    }
    }
    if (!($tr_result_175_1 =~ m{\n\z}msx || $tr_result_175_1 eq q{})) {
    $tr_result_175_1 .= "\n";
    }
    $output_175 = $tr_result_175_1;
    $output_175 = $tr_result_175_1;
    if ($output_175 ne q{} && !defined $output_printed_175) {
        print $output_175;
        if (!($output_175 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_175 ) { $main_exit_code = 1; }
    }
{
    my $output_177 = q{};
    my $output_printed_177;
    my $pipeline_success_177 = 1;
    $output_177 .= 'text with pattern in it' . "\n";
if ( !($output_177 =~ m{\n\z}msx) ) { $output_177 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_177_1;
    my @grep_lines_177_1 = split /\n/msx, $output_177;
    my @grep_filtered_177_1 = grep { /pattern/msx } @grep_lines_177_1;
    my @grep_colored_177_1;
    for my $line (@grep_filtered_177_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_177_1, $colored_line;
    }
    $grep_result_177_1 = join "\n", @grep_colored_177_1;
    if (!($grep_result_177_1 =~ m{\n\z}msx || $grep_result_177_1 eq q{})) {
    $grep_result_177_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_177_1 > 0 ? 0 : 1;
    $output_177 = $grep_result_177_1;
    $output_177 = $grep_result_177_1;
    if ((scalar @grep_filtered_177_1) == 0) {
        $pipeline_success_177 = 0;
    }
    if ($output_177 ne q{} && !defined $output_printed_177) {
        print $output_177;
        if (!($output_177 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_177 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
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
    my @grep_filtered_178 = grep { /pattern/msx } @grep_lines_178;
    $grep_result_178 = join "\n", @grep_filtered_178;
        if (!($grep_result_178 =~ m{\n\z}msx || $grep_result_178 eq q{})) {
            $grep_result_178 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_178 > 0 ? 0 : 1;
    $grep_result_178 = q{};
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

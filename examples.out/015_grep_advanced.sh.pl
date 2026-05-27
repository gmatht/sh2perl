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

# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_172 = q{};
    my $output_printed_172;
    my $pipeline_success_172 = 1;
    $output_172 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_172 =~ m{\n\z}msx) ) { $output_172 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_172_1;
    my @grep_lines_172_1 = split /\n/msx, $output_172;
    my @grep_filtered_172_1 = grep { /match/msx } @grep_lines_172_1;
    @grep_filtered_172_1 = @grep_filtered_172_1[0..1];
    $grep_result_172_1 = join "\n", @grep_filtered_172_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_173 = q{};
    my $output_printed_173;
    my $pipeline_success_173 = 1;
    $output_173 .= 'text with pattern in it' . "\n";
if ( !($output_173 =~ m{\n\z}msx) ) { $output_173 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_173_1;
    my @grep_lines_173_1 = split /\n/msx, $output_173;
    my @grep_filtered_173_1 = grep { /pattern/msx } @grep_lines_173_1;
    my @grep_with_offset_173_1;
    my $offset_173_1 = 0;
    for my $line (@grep_lines_173_1) {
    if (grep { $_ eq $line } @grep_filtered_173_1) {
    push @grep_with_offset_173_1, sprintf "%d:%s", $offset_173_1, $line;
    }
    $offset_173_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_173_1 = join "\n", @grep_with_offset_173_1;
    if (!($grep_result_173_1 =~ m{\n\z}msx || $grep_result_173_1 eq q{})) {
    $grep_result_173_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_173_1 > 0 ? 0 : 1;
    $output_173 = $grep_result_173_1;
    $output_173 = $grep_result_173_1;
    if ((scalar @grep_filtered_173_1) == 0) {
        $pipeline_success_173 = 0;
    }
    if ($output_173 ne q{} && !defined $output_printed_173) {
        print $output_173;
        if (!($output_173 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_173 ) { $main_exit_code = 1; }
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
$grep_result_174 = join "\n", @grep_filtered_174;
if (!($grep_result_174 =~ m{\n\z}msx || $grep_result_174 eq q{})) {
    $grep_result_174 .= "\n";
}
print $grep_result_174;
$CHILD_ERROR = scalar @grep_filtered_174 > 0 ? 0 : 1;
my $grep_result_175;
my @grep_lines_175 = ();
my @grep_filenames_175 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_175, $line;
        push @grep_filenames_175, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_175 = grep { /content/msx } @grep_lines_175;
my @grep_with_filename_175;
for my $line (@grep_filtered_175) {
    push @grep_with_filename_175, "temp_file.txt:$line";
}
$grep_result_175 = join "\n", @grep_with_filename_175;
if (!($grep_result_175 =~ m{\n\z}msx || $grep_result_175 eq q{})) {
    $grep_result_175 .= "\n";
}
print $grep_result_175;
$CHILD_ERROR = scalar @grep_filtered_175 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_176 = q{};
    my $output_printed_176;
    my $pipeline_success_176 = 1;
        my $grep_result_176_0;
    my @grep_lines_176_0 = ();
    my @grep_filenames_176_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_176_0, $line;
    push @grep_filenames_176_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_176_0 = grep { /pattern/msx } @grep_lines_176_0;
    $grep_result_176_0 = @grep_filtered_176_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_176_0 > 0 ? 0 : 1;
    $output_176 = $grep_result_176_0;
    $output_176 = $grep_result_176_0;
    if ((scalar @grep_filtered_176_0) == 0) {
        $pipeline_success_176 = 0;
    }

        my $set1_177 = "\\0";
    my $set2_177 = "\\n";
    my $input_177 = $output_176;
    # Expand character ranges for tr command
    my $expanded_set1_177 = $set1_177;
    my $expanded_set2_177 = $set2_177;
    # Handle a-z range in set1
    if ($expanded_set1_177 =~ /a-z/msx) {
    $expanded_set1_177 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_177 =~ /A-Z/msx) {
    $expanded_set1_177 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_177 =~ /a-z/msx) {
    $expanded_set2_177 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_177 =~ /A-Z/msx) {
    $expanded_set2_177 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_176_1 = q{};
    for my $char ( split //msx, $input_177 ) {
    my $pos_177 = index $expanded_set1_177, $char;
    if ( $pos_177 >= 0 && $pos_177 < length $expanded_set2_177 ) {
    $tr_result_176_1 .= substr $expanded_set2_177, $pos_177, 1;
    } else {
    $tr_result_176_1 .= $char;
    }
    }
    if (!($tr_result_176_1 =~ m{\n\z}msx || $tr_result_176_1 eq q{})) {
    $tr_result_176_1 .= "\n";
    }
    $output_176 = $tr_result_176_1;
    $output_176 = $tr_result_176_1;
    if ($output_176 ne q{} && !defined $output_printed_176) {
        print $output_176;
        if (!($output_176 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_176 ) { $main_exit_code = 1; }
    }
{
    my $output_178 = q{};
    my $output_printed_178;
    my $pipeline_success_178 = 1;
    $output_178 .= 'text with pattern in it' . "\n";
if ( !($output_178 =~ m{\n\z}msx) ) { $output_178 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_178_1;
    my @grep_lines_178_1 = split /\n/msx, $output_178;
    my @grep_filtered_178_1 = grep { /pattern/msx } @grep_lines_178_1;
    my @grep_colored_178_1;
    for my $line (@grep_filtered_178_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_178_1, $colored_line;
    }
    $grep_result_178_1 = join "\n", @grep_colored_178_1;
    if (!($grep_result_178_1 =~ m{\n\z}msx || $grep_result_178_1 eq q{})) {
    $grep_result_178_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_178_1 > 0 ? 0 : 1;
    $output_178 = $grep_result_178_1;
    $output_178 = $grep_result_178_1;
    if ((scalar @grep_filtered_178_1) == 0) {
        $pipeline_success_178 = 0;
    }
    if ($output_178 ne q{} && !defined $output_printed_178) {
        print $output_178;
        if (!($output_178 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_178 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_179;
    my @grep_lines_179 = ();
    my @grep_filenames_179 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_179, $line;
            push @grep_filenames_179, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_179 = grep { /pattern/msx } @grep_lines_179;
    $grep_result_179 = join "\n", @grep_filtered_179;
        if (!($grep_result_179 =~ m{\n\z}msx || $grep_result_179 eq q{})) {
            $grep_result_179 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_179 > 0 ? 0 : 1;
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

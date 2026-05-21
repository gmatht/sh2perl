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
    my $output_174 = q{};
    my $output_printed_174;
    my $pipeline_success_174 = 1;
    $output_174 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_174 =~ m{\n\z}msx) ) { $output_174 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_174_1;
    my @grep_lines_174_1 = split /\n/msx, $output_174;
    my @grep_filtered_174_1 = grep { /match/msx } @grep_lines_174_1;
    @grep_filtered_174_1 = @grep_filtered_174_1[0..1];
    $grep_result_174_1 = join "\n", @grep_filtered_174_1;
    if (!($grep_result_174_1 =~ m{\n\z}msx || $grep_result_174_1 eq q{})) {
    $grep_result_174_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_174_1 > 0 ? 0 : 1;
    $output_174 = $grep_result_174_1;
    $output_174 = $grep_result_174_1;
    if ((scalar @grep_filtered_174_1) == 0) {
        $pipeline_success_174 = 0;
    }
    if ($output_174 ne q{} && !defined $output_printed_174) {
        print $output_174;
        if (!($output_174 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_174 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
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
    my @grep_with_offset_175_1;
    my $offset_175_1 = 0;
    for my $line (@grep_lines_175_1) {
    if (grep { $_ eq $line } @grep_filtered_175_1) {
    push @grep_with_offset_175_1, sprintf "%d:%s", $offset_175_1, $line;
    }
    $offset_175_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_175_1 = join "\n", @grep_with_offset_175_1;
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
my @grep_filtered_176 = grep { /content/msx } @grep_lines_176;
$grep_result_176 = join "\n", @grep_filtered_176;
if (!($grep_result_176 =~ m{\n\z}msx || $grep_result_176 eq q{})) {
    $grep_result_176 .= "\n";
}
print $grep_result_176;
$CHILD_ERROR = scalar @grep_filtered_176 > 0 ? 0 : 1;
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
my @grep_with_filename_177;
for my $line (@grep_filtered_177) {
    push @grep_with_filename_177, "temp_file.txt:$line";
}
$grep_result_177 = join "\n", @grep_with_filename_177;
if (!($grep_result_177 =~ m{\n\z}msx || $grep_result_177 eq q{})) {
    $grep_result_177 .= "\n";
}
print $grep_result_177;
$CHILD_ERROR = scalar @grep_filtered_177 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_178 = q{};
    my $output_printed_178;
    my $pipeline_success_178 = 1;
        my $grep_result_178_0;
    my @grep_lines_178_0 = ();
    my @grep_filenames_178_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_178_0, $line;
    push @grep_filenames_178_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_178_0 = grep { /pattern/msx } @grep_lines_178_0;
    $grep_result_178_0 = @grep_filtered_178_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_178_0 > 0 ? 0 : 1;
    $output_178 = $grep_result_178_0;
    $output_178 = $grep_result_178_0;
    if ((scalar @grep_filtered_178_0) == 0) {
        $pipeline_success_178 = 0;
    }

        my $set1_179 = "\\0";
    my $set2_179 = "\\n";
    my $input_179 = $output_178;
    # Expand character ranges for tr command
    my $expanded_set1_179 = $set1_179;
    my $expanded_set2_179 = $set2_179;
    # Handle a-z range in set1
    if ($expanded_set1_179 =~ /a-z/msx) {
    $expanded_set1_179 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_179 =~ /A-Z/msx) {
    $expanded_set1_179 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_179 =~ /a-z/msx) {
    $expanded_set2_179 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_179 =~ /A-Z/msx) {
    $expanded_set2_179 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_178_1 = q{};
    for my $char ( split //msx, $input_179 ) {
    my $pos_179 = index $expanded_set1_179, $char;
    if ( $pos_179 >= 0 && $pos_179 < length $expanded_set2_179 ) {
    $tr_result_178_1 .= substr $expanded_set2_179, $pos_179, 1;
    } else {
    $tr_result_178_1 .= $char;
    }
    }
    if (!($tr_result_178_1 =~ m{\n\z}msx || $tr_result_178_1 eq q{})) {
    $tr_result_178_1 .= "\n";
    }
    $output_178 = $tr_result_178_1;
    $output_178 = $tr_result_178_1;
    if ($output_178 ne q{} && !defined $output_printed_178) {
        print $output_178;
        if (!($output_178 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_178 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep --color=always "pattern" || echo
{
    my $output_180 = q{};
    my $output_printed_180;
    my $pipeline_success_180 = 1;
    $output_180 .= 'text with pattern in it' . "\n";
if ( !($output_180 =~ m{\n\z}msx) ) { $output_180 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_180_1;
    my @grep_lines_180_1 = split /\n/msx, $output_180;
    my @grep_filtered_180_1 = grep { /pattern/msx } @grep_lines_180_1;
    my @grep_colored_180_1;
    for my $line (@grep_filtered_180_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_180_1, $colored_line;
    }
    $grep_result_180_1 = join "\n", @grep_colored_180_1;
    if (!($grep_result_180_1 =~ m{\n\z}msx || $grep_result_180_1 eq q{})) {
    $grep_result_180_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_180_1 > 0 ? 0 : 1;
    $output_180 = $grep_result_180_1;
    $output_180 = $grep_result_180_1;
    if ((scalar @grep_filtered_180_1) == 0) {
        $pipeline_success_180 = 0;
    }
    if ($output_180 ne q{} && !defined $output_printed_180) {
        print $output_180;
        if (!($output_180 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_180 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_181;
    my @grep_lines_181 = ();
    my @grep_filenames_181 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_181, $line;
            push @grep_filenames_181, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_181 = grep { /pattern/msx } @grep_lines_181;
    $grep_result_181 = join "\n", @grep_filtered_181;
        if (!($grep_result_181 =~ m{\n\z}msx || $grep_result_181 eq q{})) {
            $grep_result_181 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_181 > 0 ? 0 : 1;
    $CHILD_ERROR == 0
}) {
            print "found\n";
    if ($CHILD_ERROR != 0) {
                print "not found\n";
    }
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

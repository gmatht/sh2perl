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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '015_grep_advanced.sh';
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_180 = q{};
    my $output_printed_180;
    my $pipeline_success_180 = 1;
    $output_180 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_180 =~ m{\n\z}msx) ) { $output_180 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_180_1;
    my @grep_lines_180_1 = split /\n/msx, $output_180;
    my @grep_filtered_180_1 = grep { /match/msx } @grep_lines_180_1;
    @grep_filtered_180_1 = @grep_filtered_180_1[0..1];
    $grep_result_180_1 = join "\n", @grep_filtered_180_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
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
    my @grep_with_offset_181_1;
    my $offset_181_1 = 0;
    for my $line (@grep_lines_181_1) {
    if (grep { $_ eq $line } @grep_filtered_181_1) {
    push @grep_with_offset_181_1, sprintf "%d:%s", $offset_181_1, $line;
    }
    $offset_181_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_181_1 = join "\n", @grep_with_offset_181_1;
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
my @grep_filtered_182 = grep { /content/msx } @grep_lines_182;
$grep_result_182 = join "\n", @grep_filtered_182;
if (!($grep_result_182 =~ m{\n\z}msx || $grep_result_182 eq q{})) {
    $grep_result_182 .= "\n";
}
print $grep_result_182;
$CHILD_ERROR = scalar @grep_filtered_182 > 0 ? 0 : 1;
my $grep_result_183;
my @grep_lines_183 = ();
my @grep_filenames_183 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_183, $line;
        push @grep_filenames_183, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_183 = grep { /content/msx } @grep_lines_183;
my @grep_with_filename_183;
for my $line (@grep_filtered_183) {
    push @grep_with_filename_183, "temp_file.txt:$line";
}
$grep_result_183 = join "\n", @grep_with_filename_183;
if (!($grep_result_183 =~ m{\n\z}msx || $grep_result_183 eq q{})) {
    $grep_result_183 .= "\n";
}
print $grep_result_183;
$CHILD_ERROR = scalar @grep_filtered_183 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_184 = q{};
    my $output_printed_184;
    my $pipeline_success_184 = 1;
        my $grep_result_184_0;
    my @grep_lines_184_0 = ();
    my @grep_filenames_184_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_184_0, $line;
    push @grep_filenames_184_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_184_0 = grep { /pattern/msx } @grep_lines_184_0;
    $grep_result_184_0 = @grep_filtered_184_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_184_0 > 0 ? 0 : 1;
    $output_184 = $grep_result_184_0;
    $output_184 = $grep_result_184_0;

        my $set1_185 = "\\0";
    my $set2_185 = "\\n";
    my $input_185 = $output_184;
    # Expand character ranges for tr command
    my $expanded_set1_185 = $set1_185;
    my $expanded_set2_185 = $set2_185;
    # Handle a-z range in set1
    if ($expanded_set1_185 =~ /a-z/msx) {
    $expanded_set1_185 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_185 =~ /A-Z/msx) {
    $expanded_set1_185 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_185 =~ /\[:upper:\]/msx) {
    $expanded_set1_185 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_185 =~ /\[:lower:\]/msx) {
    $expanded_set1_185 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_185 =~ /a-z/msx) {
    $expanded_set2_185 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_185 =~ /A-Z/msx) {
    $expanded_set2_185 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_185 =~ /\[:upper:\]/msx) {
    $expanded_set2_185 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_185 =~ /\[:lower:\]/msx) {
    $expanded_set2_185 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_184_1 = q{};
    for my $char ( split //msx, $input_185 ) {
    my $pos_185 = index $expanded_set1_185, $char;
    if ( $pos_185 >= 0 && $pos_185 < length $expanded_set2_185 ) {
    $tr_result_184_1 .= substr $expanded_set2_185, $pos_185, 1;
    } else {
    $tr_result_184_1 .= $char;
    }
    }
    if (!($tr_result_184_1 =~ m{\n\z}msx || $tr_result_184_1 eq q{})) {
    $tr_result_184_1 .= "\n";
    }
    $output_184 = $tr_result_184_1;
    $output_184 = $tr_result_184_1;
    if ($output_184 ne q{} && !defined $output_printed_184) {
        print $output_184;
        if (!($output_184 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_184 ) { $main_exit_code = 1; }
    }
{
    my $output_186 = q{};
    my $output_printed_186;
    my $pipeline_success_186 = 1;
    $output_186 .= 'text with pattern in it' . "\n";
if ( !($output_186 =~ m{\n\z}msx) ) { $output_186 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_186_1;
    my @grep_lines_186_1 = split /\n/msx, $output_186;
    my @grep_filtered_186_1 = grep { /pattern/msx } @grep_lines_186_1;
    my @grep_colored_186_1;
    for my $line (@grep_filtered_186_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_186_1, $colored_line;
    }
    $grep_result_186_1 = join "\n", @grep_colored_186_1;
    if (!($grep_result_186_1 =~ m{\n\z}msx || $grep_result_186_1 eq q{})) {
    $grep_result_186_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_186_1 > 0 ? 0 : 1;
    $output_186 = $grep_result_186_1;
    $output_186 = $grep_result_186_1;
    if ((scalar @grep_filtered_186_1) == 0) {
        $pipeline_success_186 = 0;
    }
    if ($output_186 ne q{} && !defined $output_printed_186) {
        print $output_186;
        if (!($output_186 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_186 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_187;
    my @grep_lines_187 = ();
    my @grep_filenames_187 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_187, $line;
            push @grep_filenames_187, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_187 = grep { /pattern/msx } @grep_lines_187;
    $grep_result_187 = join "\n", @grep_filtered_187;
        if (!($grep_result_187 =~ m{\n\z}msx || $grep_result_187 eq q{})) {
            $grep_result_187 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_187 > 0 ? 0 : 1;
    $grep_result_187 = q{};
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

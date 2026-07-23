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
    my $output_179 = q{};
    my $output_printed_179;
    my $pipeline_success_179 = 1;
    $output_179 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_179 =~ m{\n\z}msx) ) { $output_179 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_179_1;
    my @grep_lines_179_1 = split /\n/msx, $output_179;
    my @grep_filtered_179_1 = grep { /match/msx } @grep_lines_179_1;
    @grep_filtered_179_1 = @grep_filtered_179_1[0..1];
    $grep_result_179_1 = join "\n", @grep_filtered_179_1;
    if (!($grep_result_179_1 =~ m{\n\z}msx || $grep_result_179_1 eq q{})) {
    $grep_result_179_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_179_1 > 0 ? 0 : 1;
    $output_179 = $grep_result_179_1;
    $output_179 = $grep_result_179_1;
    if ((scalar @grep_filtered_179_1) == 0) {
        $pipeline_success_179 = 0;
    }
    if ($output_179 ne q{} && !defined $output_printed_179) {
        print $output_179;
        if (!($output_179 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_179 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
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
    my @grep_with_offset_180_1;
    my $offset_180_1 = 0;
    for my $line (@grep_lines_180_1) {
    if (grep { $_ eq $line } @grep_filtered_180_1) {
    push @grep_with_offset_180_1, sprintf "%d:%s", $offset_180_1, $line;
    }
    $offset_180_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_180_1 = join "\n", @grep_with_offset_180_1;
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
my @grep_filtered_181 = grep { /content/msx } @grep_lines_181;
$grep_result_181 = join "\n", @grep_filtered_181;
if (!($grep_result_181 =~ m{\n\z}msx || $grep_result_181 eq q{})) {
    $grep_result_181 .= "\n";
}
print $grep_result_181;
$CHILD_ERROR = scalar @grep_filtered_181 > 0 ? 0 : 1;
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
my @grep_with_filename_182;
for my $line (@grep_filtered_182) {
    push @grep_with_filename_182, "temp_file.txt:$line";
}
$grep_result_182 = join "\n", @grep_with_filename_182;
if (!($grep_result_182 =~ m{\n\z}msx || $grep_result_182 eq q{})) {
    $grep_result_182 .= "\n";
}
print $grep_result_182;
$CHILD_ERROR = scalar @grep_filtered_182 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_183 = q{};
    my $output_printed_183;
    my $pipeline_success_183 = 1;
        my $grep_result_183_0;
    my @grep_lines_183_0 = ();
    my @grep_filenames_183_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_183_0, $line;
    push @grep_filenames_183_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_183_0 = grep { /pattern/msx } @grep_lines_183_0;
    $grep_result_183_0 = @grep_filtered_183_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_183_0 > 0 ? 0 : 1;
    $output_183 = $grep_result_183_0;
    $output_183 = $grep_result_183_0;

        my $set1_184 = "\\0";
    my $set2_184 = "\\n";
    my $input_184 = $output_183;
    # Expand character ranges for tr command
    my $expanded_set1_184 = $set1_184;
    my $expanded_set2_184 = $set2_184;
    # Handle a-z range in set1
    if ($expanded_set1_184 =~ /a-z/msx) {
    $expanded_set1_184 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_184 =~ /A-Z/msx) {
    $expanded_set1_184 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_184 =~ /\[:upper:\]/msx) {
    $expanded_set1_184 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_184 =~ /\[:lower:\]/msx) {
    $expanded_set1_184 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_184 =~ /a-z/msx) {
    $expanded_set2_184 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_184 =~ /A-Z/msx) {
    $expanded_set2_184 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_184 =~ /\[:upper:\]/msx) {
    $expanded_set2_184 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_184 =~ /\[:lower:\]/msx) {
    $expanded_set2_184 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_183_1 = q{};
    for my $char ( split //msx, $input_184 ) {
    my $pos_184 = index $expanded_set1_184, $char;
    if ( $pos_184 >= 0 && $pos_184 < length $expanded_set2_184 ) {
    $tr_result_183_1 .= substr $expanded_set2_184, $pos_184, 1;
    } else {
    $tr_result_183_1 .= $char;
    }
    }
    if (!($tr_result_183_1 =~ m{\n\z}msx || $tr_result_183_1 eq q{})) {
    $tr_result_183_1 .= "\n";
    }
    $output_183 = $tr_result_183_1;
    $output_183 = $tr_result_183_1;
    if ($output_183 ne q{} && !defined $output_printed_183) {
        print $output_183;
        if (!($output_183 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_183 ) { $main_exit_code = 1; }
    }
{
    my $output_185 = q{};
    my $output_printed_185;
    my $pipeline_success_185 = 1;
    $output_185 .= 'text with pattern in it' . "\n";
if ( !($output_185 =~ m{\n\z}msx) ) { $output_185 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_185_1;
    my @grep_lines_185_1 = split /\n/msx, $output_185;
    my @grep_filtered_185_1 = grep { /pattern/msx } @grep_lines_185_1;
    my @grep_colored_185_1;
    for my $line (@grep_filtered_185_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_185_1, $colored_line;
    }
    $grep_result_185_1 = join "\n", @grep_colored_185_1;
    if (!($grep_result_185_1 =~ m{\n\z}msx || $grep_result_185_1 eq q{})) {
    $grep_result_185_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_185_1 > 0 ? 0 : 1;
    $output_185 = $grep_result_185_1;
    $output_185 = $grep_result_185_1;
    if ((scalar @grep_filtered_185_1) == 0) {
        $pipeline_success_185 = 0;
    }
    if ($output_185 ne q{} && !defined $output_printed_185) {
        print $output_185;
        if (!($output_185 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_185 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_186;
    my @grep_lines_186 = ();
    my @grep_filenames_186 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_186, $line;
            push @grep_filenames_186, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_186 = grep { /pattern/msx } @grep_lines_186;
    $grep_result_186 = join "\n", @grep_filtered_186;
        if (!($grep_result_186 =~ m{\n\z}msx || $grep_result_186 eq q{})) {
            $grep_result_186 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_186 > 0 ? 0 : 1;
    $grep_result_186 = q{};
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

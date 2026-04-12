#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_181;
    my $output_printed_181;
    my $pipeline_success_181 = 1;
    $output_181 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_181 =~ m{\n\z}msx) ) { $output_181 .= "\n"; }

        my $grep_result_181_1;
    my @grep_lines_181_1 = split /\n/msx, $output_181;
    my @grep_filtered_181_1 = grep { /match/msx } @grep_lines_181_1;
    @grep_filtered_181_1 = @grep_filtered_181_1[0..1];
    $grep_result_181_1 = join "\n", @grep_filtered_181_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_182;
    my $output_printed_182;
    my $pipeline_success_182 = 1;
    $output_182 .= "text with pattern in it\n";
if ( !($output_182 =~ m{\n\z}msx) ) { $output_182 .= "\n"; }

        my $grep_result_182_1;
    my @grep_lines_182_1 = split /\n/msx, $output_182;
    my @grep_filtered_182_1 = grep { /pattern/msx } @grep_lines_182_1;
    my @grep_with_offset_182_1;
    my $offset_182_1 = 0;
    for my $line (@grep_lines_182_1) {
    if (grep { $_ eq $line } @grep_filtered_182_1) {
    push @grep_with_offset_182_1, sprintf "%d:%s", $offset_182_1, $line;
    }
    $offset_182_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_182_1 = join "\n", @grep_with_offset_182_1;
    if (!($grep_result_182_1 =~ m{\n\z}msx || $grep_result_182_1 eq q{})) {
    $grep_result_182_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_182_1 > 0 ? 0 : 1;
    $output_182 = $grep_result_182_1;
    $output_182 = $grep_result_182_1;
    if ((scalar @grep_filtered_182_1) == 0) {
        $pipeline_success_182 = 0;
    }
    if ($output_182 ne q{} && !defined $output_printed_182) {
        print $output_182;
        if (!($output_182 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_182 ) { $main_exit_code = 1; }
    }
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp_file.txt'
      or die "Cannot open file: $!\n";
    print "content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
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
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_183 = grep { /content/msx } @grep_lines_183;
$grep_result_183 = join "\n", @grep_filtered_183;
if (!($grep_result_183 =~ m{\n\z}msx || $grep_result_183 eq q{})) {
    $grep_result_183 .= "\n";
}
print $grep_result_183;
$CHILD_ERROR = scalar @grep_filtered_183 > 0 ? 0 : 1;
my $grep_result_184;
my @grep_lines_184 = ();
my @grep_filenames_184 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_184, $line;
        push @grep_filenames_184, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_184 = grep { /content/msx } @grep_lines_184;
my @grep_with_filename_184;
for my $line (@grep_filtered_184) {
    push @grep_with_filename_184, "temp_file.txt:$line";
}
$grep_result_184 = join "\n", @grep_with_filename_184;
if (!($grep_result_184 =~ m{\n\z}msx || $grep_result_184 eq q{})) {
    $grep_result_184 .= "\n";
}
print $grep_result_184;
$CHILD_ERROR = scalar @grep_filtered_184 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_185;
    my $output_printed_185;
    my $pipeline_success_185 = 1;
        my $grep_result_185_0;
    my @grep_lines_185_0 = ();
    my @grep_filenames_185_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_185_0, $line;
    push @grep_filenames_185_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_185_0 = grep { /pattern/msx } @grep_lines_185_0;
    $grep_result_185_0 = @grep_filtered_185_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_185_0 > 0 ? 0 : 1;
    $output_185 = $grep_result_185_0;
    $output_185 = $grep_result_185_0;
    if ((scalar @grep_filtered_185_0) == 0) {
        $pipeline_success_185 = 0;
    }

        my $set1_186 = "\\0";
    my $set2_186 = "\\n";
    my $input_186 = $output_185;
    # Expand character ranges for tr command
    my $expanded_set1_186 = $set1_186;
    my $expanded_set2_186 = $set2_186;
    # Handle a-z range in set1
    if ($expanded_set1_186 =~ /a-z/msx) {
    $expanded_set1_186 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_186 =~ /A-Z/msx) {
    $expanded_set1_186 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_186 =~ /a-z/msx) {
    $expanded_set2_186 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_186 =~ /A-Z/msx) {
    $expanded_set2_186 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_185_1 = q{};
    for my $char ( split //msx, $input_186 ) {
    my $pos_186 = index $expanded_set1_186, $char;
    if ( $pos_186 >= 0 && $pos_186 < length $expanded_set2_186 ) {
    $tr_result_185_1 .= substr $expanded_set2_186, $pos_186, 1;
    } else {
    $tr_result_185_1 .= $char;
    }
    }
    if (!($tr_result_185_1 =~ m{\n\z}msx || $tr_result_185_1 eq q{})) {
    $tr_result_185_1 .= "\n";
    }
    $output_185 = $tr_result_185_1;
    if ($output_185 ne q{} && !defined $output_printed_185) {
        print $output_185;
        if (!($output_185 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_185 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep --color=always "pattern" || echo "Color not supported"
{
    my $output_187;
    my $output_printed_187;
    my $pipeline_success_187 = 1;
    $output_187 .= "text with pattern in it\n";
if ( !($output_187 =~ m{\n\z}msx) ) { $output_187 .= "\n"; }

        my $exit_code_188;
    {
    my $temp_input_188 = $output_187;
    my $grep_result_188;
    my @grep_lines_188 = split /\n/msx, $temp_input_188;
    my @grep_filtered_188 = grep { /pattern/msx } @grep_lines_188;
    my @grep_colored_188;
    for my $line (@grep_filtered_188) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_188, $colored_line;
    }
    $grep_result_188 = join "\n", @grep_colored_188;
    if (!($grep_result_188 =~ m{\n\z}msx || $grep_result_188 eq q{})) {
    $grep_result_188 .= "\n";
    }
    print $grep_result_188;
    $CHILD_ERROR = scalar @grep_filtered_188 > 0 ? 0 : 1;
    $exit_code_188 = $CHILD_ERROR;
    }
    if ($exit_code_188 != 0) {
    print "Color not supported\n";
    } else {
    $output_printed_187 = 1;  # Mark as printed to avoid double output
    }
    if ($output_187 ne q{} && !defined $output_printed_187) {
        print $output_187;
        if (!($output_187 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_187 ) { $main_exit_code = 1; }
    }
if (do {
        my $grep_result_189;
    my @grep_lines_189 = ();
    my @grep_filenames_189 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_189, $line;
            push @grep_filenames_189, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_189 = grep { /pattern/msx } @grep_lines_189;
    $grep_result_189 = join "\n", @grep_filtered_189;
        if (!($grep_result_189 =~ m{\n\z}msx || $grep_result_189 eq q{})) {
            $grep_result_189 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_189 > 0 ? 0 : 1;
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

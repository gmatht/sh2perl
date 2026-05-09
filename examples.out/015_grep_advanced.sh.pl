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
    my $output_173 = q{};
    my $output_printed_173;
    my $pipeline_success_173 = 1;
    $output_173 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_173 =~ m{\n\z}msx) ) { $output_173 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_173_1;
    my @grep_lines_173_1 = split /\n/msx, $output_173;
    my @grep_filtered_173_1 = grep { /match/msx } @grep_lines_173_1;
    @grep_filtered_173_1 = @grep_filtered_173_1[0..1];
    $grep_result_173_1 = join "\n", @grep_filtered_173_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_174 = q{};
    my $output_printed_174;
    my $pipeline_success_174 = 1;
    $output_174 .= 'text with pattern in it' . "\n";
if ( !($output_174 =~ m{\n\z}msx) ) { $output_174 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_174_1;
    my @grep_lines_174_1 = split /\n/msx, $output_174;
    my @grep_filtered_174_1 = grep { /pattern/msx } @grep_lines_174_1;
    my @grep_with_offset_174_1;
    my $offset_174_1 = 0;
    for my $line (@grep_lines_174_1) {
    if (grep { $_ eq $line } @grep_filtered_174_1) {
    push @grep_with_offset_174_1, sprintf "%d:%s", $offset_174_1, $line;
    }
    $offset_174_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_174_1 = join "\n", @grep_with_offset_174_1;
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
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_175 = grep { /content/msx } @grep_lines_175;
$grep_result_175 = join "\n", @grep_filtered_175;
if (!($grep_result_175 =~ m{\n\z}msx || $grep_result_175 eq q{})) {
    $grep_result_175 .= "\n";
}
print $grep_result_175;
$CHILD_ERROR = scalar @grep_filtered_175 > 0 ? 0 : 1;
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
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_176 = grep { /content/msx } @grep_lines_176;
my @grep_with_filename_176;
for my $line (@grep_filtered_176) {
    push @grep_with_filename_176, "temp_file.txt:$line";
}
$grep_result_176 = join "\n", @grep_with_filename_176;
if (!($grep_result_176 =~ m{\n\z}msx || $grep_result_176 eq q{})) {
    $grep_result_176 .= "\n";
}
print $grep_result_176;
$CHILD_ERROR = scalar @grep_filtered_176 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_177 = q{};
    my $output_printed_177;
    my $pipeline_success_177 = 1;
        my $grep_result_177_0;
    my @grep_lines_177_0 = ();
    my @grep_filenames_177_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_177_0, $line;
    push @grep_filenames_177_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_177_0 = grep { /pattern/msx } @grep_lines_177_0;
    $grep_result_177_0 = @grep_filtered_177_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_177_0 > 0 ? 0 : 1;
    $output_177 = $grep_result_177_0;
    $output_177 = $grep_result_177_0;

        my $set1_178 = "\\0";
    my $set2_178 = "\\n";
    my $input_178 = $output_177;
    # Expand character ranges for tr command
    my $expanded_set1_178 = $set1_178;
    my $expanded_set2_178 = $set2_178;
    # Handle a-z range in set1
    if ($expanded_set1_178 =~ /a-z/msx) {
    $expanded_set1_178 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_178 =~ /A-Z/msx) {
    $expanded_set1_178 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_178 =~ /a-z/msx) {
    $expanded_set2_178 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_178 =~ /A-Z/msx) {
    $expanded_set2_178 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_177_1 = q{};
    for my $char ( split //msx, $input_178 ) {
    my $pos_178 = index $expanded_set1_178, $char;
    if ( $pos_178 >= 0 && $pos_178 < length $expanded_set2_178 ) {
    $tr_result_177_1 .= substr $expanded_set2_178, $pos_178, 1;
    } else {
    $tr_result_177_1 .= $char;
    }
    }
    if (!($tr_result_177_1 =~ m{\n\z}msx || $tr_result_177_1 eq q{})) {
    $tr_result_177_1 .= "\n";
    }
    $output_177 = $tr_result_177_1;
    $output_177 = $tr_result_177_1;
    if ($output_177 ne q{} && !defined $output_printed_177) {
        print $output_177;
        if (!($output_177 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_177 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep --color=always "pattern" || echo
{
    my $output_179 = q{};
    my $output_printed_179;
    my $pipeline_success_179 = 1;
    $output_179 .= 'text with pattern in it' . "\n";
if ( !($output_179 =~ m{\n\z}msx) ) { $output_179 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_179_1;
    my @grep_lines_179_1 = split /\n/msx, $output_179;
    my @grep_filtered_179_1 = grep { /pattern/msx } @grep_lines_179_1;
    my @grep_colored_179_1;
    for my $line (@grep_filtered_179_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_179_1, $colored_line;
    }
    $grep_result_179_1 = join "\n", @grep_colored_179_1;
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
if ($CHILD_ERROR != 0) {
        print "Color not supported\n";
}
if (do {
        my $grep_result_180;
    my @grep_lines_180 = ();
    my @grep_filenames_180 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_180, $line;
            push @grep_filenames_180, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_180 = grep { /pattern/msx } @grep_lines_180;
    $grep_result_180 = join "\n", @grep_filtered_180;
        if (!($grep_result_180 =~ m{\n\z}msx || $grep_result_180 eq q{})) {
            $grep_result_180 .= "\n";
        }
    $grep_result_180 = q{};
    $CHILD_ERROR = scalar @grep_filtered_180 > 0 ? 0 : 1;
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

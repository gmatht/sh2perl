#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '015_grep_advanced.sh';
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
do {
    my $output_168 = q{};
    my $output_printed_168;
    my $pipeline_success_168 = 1;
    $output_168 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_168 =~ m{\n\z}) ) { $output_168 .= "\n"; }

        my $grep_result_168_1;
    my @grep_lines_168_1 = split /\n/msx, $output_168;
    my @grep_filtered_168_1 = grep { /match/msx } @grep_lines_168_1;
    @grep_filtered_168_1 = @grep_filtered_168_1[0..1];
    $grep_result_168_1 = join "\n", @grep_filtered_168_1;
    if (!($grep_result_168_1 =~ m{\n\z} || $grep_result_168_1 eq q{})) {
    $grep_result_168_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_168_1 > 0 ? 0 : 1;
    $output_168 = $grep_result_168_1;
    $output_168 = $grep_result_168_1;
    if ((scalar @grep_filtered_168_1) == 0) {
        $pipeline_success_168 = 0;
    }
    if ($output_168 ne q{} && !defined $output_printed_168) {
        print $output_168;
        if (!($output_168 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_168 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
do {
    my $output_169 = q{};
    my $output_printed_169;
    my $pipeline_success_169 = 1;
    $output_169 .= 'text with pattern in it' . "\n";
if ( !($output_169 =~ m{\n\z}) ) { $output_169 .= "\n"; }

        my $grep_result_169_1;
    my @grep_lines_169_1 = split /\n/msx, $output_169;
    my @grep_filtered_169_1 = grep { /pattern/msx } @grep_lines_169_1;
    my @grep_with_offset_169_1;
    my $offset_169_1 = 0;
    for my $line (@grep_lines_169_1) {
    if (grep { $_ eq $line } @grep_filtered_169_1) {
    push @grep_with_offset_169_1, sprintf "%d:%s", $offset_169_1, $line;
    }
    $offset_169_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_169_1 = join "\n", @grep_with_offset_169_1;
    if (!($grep_result_169_1 =~ m{\n\z} || $grep_result_169_1 eq q{})) {
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
        if (!($output_169 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_169 ) { $main_exit_code = 1; }
    }
open my $fh, '>', 'temp_file.txt' or die "temp_file.txt: $!\n";
say {*fh} "content";
close $fh;
my $grep_result_170;
my @grep_lines_170 = ();
my @grep_filenames_170 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_170, $line;
        push @grep_filenames_170, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_170 = grep { /content/msx } @grep_lines_170;
$grep_result_170 = join "\n", @grep_filtered_170;
if (!($grep_result_170 =~ m{\n\z} || $grep_result_170 eq q{})) {
    $grep_result_170 .= "\n";
}
print $grep_result_170;
$CHILD_ERROR = scalar @grep_filtered_170 > 0 ? 0 : 1;
my $grep_result_171;
my @grep_lines_171 = ();
my @grep_filenames_171 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
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
my @grep_with_filename_171;
for my $line (@grep_filtered_171) {
    push @grep_with_filename_171, "temp_file.txt:$line";
}
$grep_result_171 = join "\n", @grep_with_filename_171;
if (!($grep_result_171 =~ m{\n\z} || $grep_result_171 eq q{})) {
    $grep_result_171 .= "\n";
}
print $grep_result_171;
$CHILD_ERROR = scalar @grep_filtered_171 > 0 ? 0 : 1;
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
do {
    my $output_172 = q{};
    my $output_printed_172;
    my $pipeline_success_172 = 1;
        my $grep_result_172_0;
    my @grep_lines_172_0 = ();
    my @grep_filenames_172_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_172_0, $line;
    push @grep_filenames_172_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_172_0 = grep { /pattern/msx } @grep_lines_172_0;
    $grep_result_172_0 = @grep_filtered_172_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_172_0 > 0 ? 0 : 1;
    $output_172 = $grep_result_172_0;
    $output_172 = $grep_result_172_0;

        my $set1_173 = "\\0";
    my $set2_173 = "\\n";
    my $input_173 = $output_172;
    # Expand character ranges for tr command
    my $expanded_set1_173 = $set1_173;
    my $expanded_set2_173 = $set2_173;
    # Handle a-z range in set1
    if ($expanded_set1_173 =~ /a-z/msx) {
    $expanded_set1_173 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_173 =~ /A-Z/msx) {
    $expanded_set1_173 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_173 =~ /\[:upper:\]/msx) {
    $expanded_set1_173 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_173 =~ /\[:lower:\]/msx) {
    $expanded_set1_173 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_173 =~ /a-z/msx) {
    $expanded_set2_173 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_173 =~ /A-Z/msx) {
    $expanded_set2_173 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_173 =~ /\[:upper:\]/msx) {
    $expanded_set2_173 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_173 =~ /\[:lower:\]/msx) {
    $expanded_set2_173 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_172_1 = q{};
    for my $char ( split //msx, $input_173 ) {
    my $pos_173 = index $expanded_set1_173, $char;
    if ( $pos_173 >= 0 && $pos_173 < length $expanded_set2_173 ) {
    $tr_result_172_1 .= substr $expanded_set2_173, $pos_173, 1;
    } else {
    $tr_result_172_1 .= $char;
    }
    }
    if (!($tr_result_172_1 =~ m{\n\z} || $tr_result_172_1 eq q{})) {
    $tr_result_172_1 .= "\n";
    }
    $output_172 = $tr_result_172_1;
    $output_172 = $tr_result_172_1;
    if ($output_172 ne q{} && !defined $output_printed_172) {
        print $output_172;
        if (!($output_172 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_172 ) { $main_exit_code = 1; }
    }
do {
    my $output_174 = q{};
    my $output_printed_174;
    my $pipeline_success_174 = 1;
    $output_174 .= 'text with pattern in it' . "\n";
if ( !($output_174 =~ m{\n\z}) ) { $output_174 .= "\n"; }

        my $grep_result_174_1;
    my @grep_lines_174_1 = split /\n/msx, $output_174;
    my @grep_filtered_174_1 = grep { /pattern/msx } @grep_lines_174_1;
    my @grep_colored_174_1;
    for my $line (@grep_filtered_174_1) {
    my $colored_line = $line;
    $colored_line =~ s/(pattern)/\x1b[01;31m\x1b[K$1\x1b[m\x1b[K/gs;
    push @grep_colored_174_1, $colored_line;
    }
    $grep_result_174_1 = join "\n", @grep_colored_174_1;
    if (!($grep_result_174_1 =~ m{\n\z} || $grep_result_174_1 eq q{})) {
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
        if (!($output_174 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_174 ) { $main_exit_code = 1; }
    }
if ($CHILD_ERROR != 0) {
        say "Color not supported";
}
if (do {
        my $grep_result_175;
    my @grep_lines_175 = ();
    my @grep_filenames_175 = ();
    if (-e "temp_file.txt") {
        open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_175, $line;
            push @grep_filenames_175, "temp_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_175 = grep { /pattern/msx } @grep_lines_175;
    $grep_result_175 = join "\n", @grep_filtered_175;
        if (!($grep_result_175 =~ m{\n\z} || $grep_result_175 eq q{})) {
            $grep_result_175 .= "\n";
        }
    $CHILD_ERROR = scalar @grep_filtered_175 > 0 ? 0 : 1;
    $grep_result_175 = q{};
    $CHILD_ERROR == 0
}) {
        say "found";
}
if ($CHILD_ERROR != 0) {
        say "not found";
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

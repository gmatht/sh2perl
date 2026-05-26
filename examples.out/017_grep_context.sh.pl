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

# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { /TARGET/msx } @grep_lines_0_1;
    my @grep_with_context_0_1;
    for my $i (0..@grep_lines_0_1-1) {
    if (scalar grep { $_ eq $grep_lines_0_1[$i] } @grep_filtered_0_1) {
    push @grep_with_context_0_1, $grep_lines_0_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_0_1, $grep_lines_0_1[$j];
    }
    }
    }
    $grep_result_0_1 = join "\n", @grep_with_context_0_1;
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_1 = q{};
    my $output_printed_1;
    my $pipeline_success_1 = 1;
    $output_1 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_1 =~ m{\n\z}msx) ) { $output_1 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_1_1;
    my @grep_lines_1_1 = split /\n/msx, $output_1;
    my @grep_filtered_1_1 = grep { /TARGET/msx } @grep_lines_1_1;
    my @grep_with_context_1_1;
    for my $i (0..@grep_lines_1_1-1) {
    if (scalar grep { $_ eq $grep_lines_1_1[$i] } @grep_filtered_1_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_1_1, $grep_lines_1_1[$j];
    }
    }
    push @grep_with_context_1_1, $grep_lines_1_1[$i];
    }
    }
    $grep_result_1_1 = join "\n", @grep_with_context_1_1;
    $CHILD_ERROR = scalar @grep_filtered_1_1 > 0 ? 0 : 1;
    $output_1 = $grep_result_1_1;
    $output_1 = $grep_result_1_1;
    if ((scalar @grep_filtered_1_1) == 0) {
        $pipeline_success_1 = 0;
    }
    if ($output_1 ne q{} && !defined $output_printed_1) {
        print $output_1;
        if (!($output_1 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_2 = q{};
    my $output_printed_2;
    my $pipeline_success_2 = 1;
    $output_2 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_2 =~ m{\n\z}msx) ) { $output_2 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_2_1;
    my @grep_lines_2_1 = split /\n/msx, $output_2;
    my @grep_filtered_2_1 = grep { /TARGET/msx } @grep_lines_2_1;
    my @grep_with_context_2_1;
    for my $i (0..@grep_lines_2_1-1) {
    if (scalar grep { $_ eq $grep_lines_2_1[$i] } @grep_filtered_2_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_2_1, $grep_lines_2_1[$j];
    }
    }
    push @grep_with_context_2_1, $grep_lines_2_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_2_1, $grep_lines_2_1[$j];
    }
    }
    }
    $grep_result_2_1 = join "\n", @grep_with_context_2_1;
    $CHILD_ERROR = scalar @grep_filtered_2_1 > 0 ? 0 : 1;
    $output_2 = $grep_result_2_1;
    $output_2 = $grep_result_2_1;
    if ((scalar @grep_filtered_2_1) == 0) {
        $pipeline_success_2 = 0;
    }
    if ($output_2 ne q{} && !defined $output_printed_2) {
        print $output_2;
        if (!($output_2 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_2 ) { $main_exit_code = 1; }
    }
print "Creating test files...\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp_file1.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "pattern in file1\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp_file2.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "no pattern in file2\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp_file3.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "pattern in file3\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
print "Recursive search results:\n";
my $grep_result_3;
my @grep_lines_3 = ();
my @grep_filenames_3 = ();
sub find_files_recursive_3 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_3($path, $pattern));
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
my @files_3 = find_files_recursive_3('.', '*.txt');
for my $file (@files_3) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_3, $line;
            push @grep_filenames_3, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_3 = grep { /pattern/msx } @grep_lines_3;
my @grep_with_filename_3;
for my $i (0..@grep_lines_3-1) {
    if (scalar grep { $_ eq $grep_lines_3[$i] } @grep_filtered_3) {
        push @grep_with_filename_3, "$grep_filenames_3[$i]:$grep_lines_3[$i]";
    }
}
$grep_result_3 = join "\n", @grep_with_filename_3;
if (!($grep_result_3 =~ m{\n\z}msx || $grep_result_3 eq q{})) {
    $grep_result_3 .= "\n";
}
print $grep_result_3;
$CHILD_ERROR = scalar @grep_filtered_3 > 0 ? 0 : 1;
print 'Result' . q{ } . '2...' . "\n";
$CHILD_ERROR = 0;
# Original bash: grep -l "pattern" *.txt | sort
{
    my $output_4 = q{};
    my $output_printed_4;
    my $pipeline_success_4 = 1;
        my $grep_result_4_0;
    my @grep_lines_4_0 = ();
    my @grep_filenames_4_0 = ();
    my @glob_files_4_0 = glob('*.txt');
    for my $glob_file (@glob_files_4_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_4_0, $line;
    push @grep_filenames_4_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_4_0 = grep { /pattern/msx } @grep_lines_4_0;
    my @matching_files_4_0;
    my %file_has_match_4_0;
    for my $i (0..@grep_lines_4_0-1) {
    if (scalar grep { $_ eq $grep_lines_4_0[$i] } @grep_filtered_4_0) {
    $file_has_match_4_0{$grep_filenames_4_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_4_0) {
    push @matching_files_4_0, $file;
    }
    $grep_result_4_0 = join "\n", @matching_files_4_0;
    $CHILD_ERROR = scalar @grep_filtered_4_0 > 0 ? 0 : 1;
    $output_4 = $grep_result_4_0;
    $output_4 = $grep_result_4_0;
    if ((scalar @grep_filtered_4_0) == 0) {
        $pipeline_success_4 = 0;
    }

        my @sort_lines_4_1 = split /\n/msx, $output_4;
    my @sort_sorted_4_1 = sort @sort_lines_4_1;
    my $output_4_1 = join "\n", @sort_sorted_4_1;
    if ($output_4_1 ne q{} && !($output_4_1 =~ m{\n\z}msx)) {
    $output_4_1 .= "\n";
    }
    $output_4 = $output_4_1;
    $output_4 = $output_4_1;
    if ($output_4 ne q{} && !defined $output_printed_4) {
        print $output_4;
        if (!($output_4 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_4 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_5;
my @grep_lines_5 = ();
my @grep_filenames_5 = ();
my @glob_files_5 = glob('*.txt');
for my $glob_file (@glob_files_5) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_5, $line;
            push @grep_filenames_5, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_5 = grep { /pattern/msx } @grep_lines_5;
my @non_matching_files_5;
my %file_has_match_5;
my %all_files_5;
my @all_glob_files_5 = glob('*.txt');
for my $file (@all_glob_files_5) {
    if (-f $file) {
        $all_files_5{$file} = 1;
    }
}
for my $i (0..@grep_lines_5-1) {
    if (scalar grep { $_ eq $grep_lines_5[$i] } @grep_filtered_5) {
        $file_has_match_5{$grep_filenames_5[$i]} = 1;
    }
}
for my $file (sort keys %all_files_5) {
    if (!exists $file_has_match_5{$file}) {
        push @non_matching_files_5, $file;
    }
}
$grep_result_5 = join "\n", @non_matching_files_5;
print $grep_result_5;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_5 > 0 ? 0 : 1;
my @files_to_remove = glob("temp_file*.txt");
foreach my $file_to_remove (@files_to_remove) {
    if ( -e $file_to_remove ) {
        if ( -d $file_to_remove ) {
            croak "rm: ", $file_to_remove,
    " is a directory (use -r to remove recursively)\n";
        }
        else {
            if ( unlink $file_to_remove ) {
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: cannot remove ", $file_to_remove,
    ": $OS_ERROR\n";
            }
        }
    }
    else {
        local $CHILD_ERROR = 1;
        croak "rm: ", $file_to_remove,
    ": No such file or directory\n";
    }
}

exit $main_exit_code;

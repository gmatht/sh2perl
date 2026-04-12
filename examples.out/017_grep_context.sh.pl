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

# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_196;
    my $output_printed_196;
    my $pipeline_success_196 = 1;
    $output_196 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_196 =~ m{\n\z}msx) ) { $output_196 .= "\n"; }

        my $grep_result_196_1;
    my @grep_lines_196_1 = split /\n/msx, $output_196;
    my @grep_filtered_196_1 = grep { /TARGET/msx } @grep_lines_196_1;
    my @grep_with_context_196_1;
    for my $i (0..@grep_lines_196_1-1) {
    if (scalar grep { $_ eq $grep_lines_196_1[$i] } @grep_filtered_196_1) {
    push @grep_with_context_196_1, $grep_lines_196_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_196_1, $grep_lines_196_1[$j];
    }
    }
    }
    $grep_result_196_1 = join "\n", @grep_with_context_196_1;
    $CHILD_ERROR = scalar @grep_filtered_196_1 > 0 ? 0 : 1;
    $output_196 = $grep_result_196_1;
    $output_196 = $grep_result_196_1;
    if ((scalar @grep_filtered_196_1) == 0) {
        $pipeline_success_196 = 0;
    }
    if ($output_196 ne q{} && !defined $output_printed_196) {
        print $output_196;
        if (!($output_196 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_196 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_197;
    my $output_printed_197;
    my $pipeline_success_197 = 1;
    $output_197 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_197 =~ m{\n\z}msx) ) { $output_197 .= "\n"; }

        my $grep_result_197_1;
    my @grep_lines_197_1 = split /\n/msx, $output_197;
    my @grep_filtered_197_1 = grep { /TARGET/msx } @grep_lines_197_1;
    my @grep_with_context_197_1;
    for my $i (0..@grep_lines_197_1-1) {
    if (scalar grep { $_ eq $grep_lines_197_1[$i] } @grep_filtered_197_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_197_1, $grep_lines_197_1[$j];
    }
    }
    push @grep_with_context_197_1, $grep_lines_197_1[$i];
    }
    }
    $grep_result_197_1 = join "\n", @grep_with_context_197_1;
    $CHILD_ERROR = scalar @grep_filtered_197_1 > 0 ? 0 : 1;
    $output_197 = $grep_result_197_1;
    $output_197 = $grep_result_197_1;
    if ((scalar @grep_filtered_197_1) == 0) {
        $pipeline_success_197 = 0;
    }
    if ($output_197 ne q{} && !defined $output_printed_197) {
        print $output_197;
        if (!($output_197 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_197 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_198;
    my $output_printed_198;
    my $pipeline_success_198 = 1;
    $output_198 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_198 =~ m{\n\z}msx) ) { $output_198 .= "\n"; }

        my $grep_result_198_1;
    my @grep_lines_198_1 = split /\n/msx, $output_198;
    my @grep_filtered_198_1 = grep { /TARGET/msx } @grep_lines_198_1;
    my @grep_with_context_198_1;
    for my $i (0..@grep_lines_198_1-1) {
    if (scalar grep { $_ eq $grep_lines_198_1[$i] } @grep_filtered_198_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_198_1, $grep_lines_198_1[$j];
    }
    }
    push @grep_with_context_198_1, $grep_lines_198_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_198_1, $grep_lines_198_1[$j];
    }
    }
    }
    $grep_result_198_1 = join "\n", @grep_with_context_198_1;
    $CHILD_ERROR = scalar @grep_filtered_198_1 > 0 ? 0 : 1;
    $output_198 = $grep_result_198_1;
    $output_198 = $grep_result_198_1;
    if ((scalar @grep_filtered_198_1) == 0) {
        $pipeline_success_198 = 0;
    }
    if ($output_198 ne q{} && !defined $output_printed_198) {
        print $output_198;
        if (!($output_198 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_198 ) { $main_exit_code = 1; }
    }
print "Creating test files...\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp_file1.txt'
      or die "Cannot open file: $!\n";
    print "pattern in file1\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp_file2.txt'
      or die "Cannot open file: $!\n";
    print "no pattern in file2\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp_file3.txt'
      or die "Cannot open file: $!\n";
    print "pattern in file3\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
print "Recursive search results:\n";
my $grep_result_199;
my @grep_lines_199 = ();
my @grep_filenames_199 = ();
sub find_files_recursive_199 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_199($path, $pattern));
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
my @files_199 = find_files_recursive_199('.', '*.txt');
for my $file (@files_199) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_199, $line;
            push @grep_filenames_199, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_199 = grep { /pattern/msx } @grep_lines_199;
my @grep_with_filename_199;
for my $i (0..@grep_lines_199-1) {
    if (scalar grep { $_ eq $grep_lines_199[$i] } @grep_filtered_199) {
        push @grep_with_filename_199, "$grep_filenames_199[$i]:$grep_lines_199[$i]";
    }
}
$grep_result_199 = join "\n", @grep_with_filename_199;
if (!($grep_result_199 =~ m{\n\z}msx || $grep_result_199 eq q{})) {
    $grep_result_199 .= "\n";
}
print $grep_result_199;
$CHILD_ERROR = scalar @grep_filtered_199 > 0 ? 0 : 1;
print 'Result' . q{ } . '2...' . "\n";
# Original bash: grep -l "pattern" *.txt | sort
{
    my $output_200;
    my $output_printed_200;
    my $pipeline_success_200 = 1;
        my $grep_result_200_0;
    my @grep_lines_200_0 = ();
    my @grep_filenames_200_0 = ();
    my @glob_files_200_0 = glob('*.txt');
    for my $glob_file (@glob_files_200_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_200_0, $line;
    push @grep_filenames_200_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_200_0 = grep { /pattern/msx } @grep_lines_200_0;
    my @matching_files_200_0;
    my %file_has_match_200_0;
    for my $i (0..@grep_lines_200_0-1) {
    if (scalar grep { $_ eq $grep_lines_200_0[$i] } @grep_filtered_200_0) {
    $file_has_match_200_0{$grep_filenames_200_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_200_0) {
    push @matching_files_200_0, $file;
    }
    $grep_result_200_0 = join "\n", @matching_files_200_0;
    $CHILD_ERROR = scalar @grep_filtered_200_0 > 0 ? 0 : 1;
    $output_200 = $grep_result_200_0;
    $output_200 = $grep_result_200_0;
    if ((scalar @grep_filtered_200_0) == 0) {
        $pipeline_success_200 = 0;
    }

        my @sort_lines_200_1 = split /\n/msx, $output_200;
    my @sort_sorted_200_1 = sort @sort_lines_200_1;
    my $output_200_1 = join "\n", @sort_sorted_200_1;
    if ($output_200_1 ne q{} && !($output_200_1 =~ m{\n\z}msx)) {
    $output_200_1 .= "\n";
    }
    $output_200 = $output_200_1;
    $output_200 = $output_200_1;
    if ($output_200 ne q{} && !defined $output_printed_200) {
        print $output_200;
        if (!($output_200 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_200 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
my $grep_result_201;
my @grep_lines_201 = ();
my @grep_filenames_201 = ();
my @glob_files_201 = glob('*.txt');
for my $glob_file (@glob_files_201) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_201, $line;
            push @grep_filenames_201, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_201 = grep { /pattern/msx } @grep_lines_201;
my @non_matching_files_201;
my %file_has_match_201;
my %all_files_201;
my @all_glob_files_201 = glob('*.txt');
for my $file (@all_glob_files_201) {
    if (-f $file) {
        $all_files_201{$file} = 1;
    }
}
for my $i (0..@grep_lines_201-1) {
    if (scalar grep { $_ eq $grep_lines_201[$i] } @grep_filtered_201) {
        $file_has_match_201{$grep_filenames_201[$i]} = 1;
    }
}
for my $file (sort keys %all_files_201) {
    if (!exists $file_has_match_201{$file}) {
        push @non_matching_files_201, $file;
    }
}
$grep_result_201 = join "\n", @non_matching_files_201;
print $grep_result_201;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_201 > 0 ? 0 : 1;
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

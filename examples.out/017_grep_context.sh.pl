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

$PROGRAM_NAME = '017_grep_context.sh';
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_194 = q{};
    my $output_printed_194;
    my $pipeline_success_194 = 1;
    $output_194 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_194 =~ m{\n\z}msx) ) { $output_194 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_194_1;
    my @grep_lines_194_1 = split /\n/msx, $output_194;
    my @grep_filtered_194_1 = grep { /TARGET/msx } @grep_lines_194_1;
    my @grep_with_context_194_1;
    for my $i (0..@grep_lines_194_1-1) {
    if (scalar grep { $_ eq $grep_lines_194_1[$i] } @grep_filtered_194_1) {
    push @grep_with_context_194_1, $grep_lines_194_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_194_1, $grep_lines_194_1[$j];
    }
    }
    }
    $grep_result_194_1 = join "\n", @grep_with_context_194_1;
    $CHILD_ERROR = scalar @grep_filtered_194_1 > 0 ? 0 : 1;
    $output_194 = $grep_result_194_1;
    $output_194 = $grep_result_194_1;
    if ((scalar @grep_filtered_194_1) == 0) {
        $pipeline_success_194 = 0;
    }
    if ($output_194 ne q{} && !defined $output_printed_194) {
        print $output_194;
        if (!($output_194 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_194 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_195 = q{};
    my $output_printed_195;
    my $pipeline_success_195 = 1;
    $output_195 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_195 =~ m{\n\z}msx) ) { $output_195 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_195_1;
    my @grep_lines_195_1 = split /\n/msx, $output_195;
    my @grep_filtered_195_1 = grep { /TARGET/msx } @grep_lines_195_1;
    my @grep_with_context_195_1;
    for my $i (0..@grep_lines_195_1-1) {
    if (scalar grep { $_ eq $grep_lines_195_1[$i] } @grep_filtered_195_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_195_1, $grep_lines_195_1[$j];
    }
    }
    push @grep_with_context_195_1, $grep_lines_195_1[$i];
    }
    }
    $grep_result_195_1 = join "\n", @grep_with_context_195_1;
    $CHILD_ERROR = scalar @grep_filtered_195_1 > 0 ? 0 : 1;
    $output_195 = $grep_result_195_1;
    $output_195 = $grep_result_195_1;
    if ((scalar @grep_filtered_195_1) == 0) {
        $pipeline_success_195 = 0;
    }
    if ($output_195 ne q{} && !defined $output_printed_195) {
        print $output_195;
        if (!($output_195 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_195 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_196 = q{};
    my $output_printed_196;
    my $pipeline_success_196 = 1;
    $output_196 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_196 =~ m{\n\z}msx) ) { $output_196 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_196_1;
    my @grep_lines_196_1 = split /\n/msx, $output_196;
    my @grep_filtered_196_1 = grep { /TARGET/msx } @grep_lines_196_1;
    my @grep_with_context_196_1;
    for my $i (0..@grep_lines_196_1-1) {
    if (scalar grep { $_ eq $grep_lines_196_1[$i] } @grep_filtered_196_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_196_1, $grep_lines_196_1[$j];
    }
    }
    push @grep_with_context_196_1, $grep_lines_196_1[$i];
    for my $j (($i + 1)..($i + 1)) {
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
my $grep_result_197;
my @grep_lines_197 = ();
my @grep_filenames_197 = ();
my $find_files_recursive_197;
$find_files_recursive_197 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_197->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_197 = $find_files_recursive_197->('.', '*.txt');
for my $file (@files_197) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_197, $line;
            push @grep_filenames_197, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_197 = grep { /pattern/msx } @grep_lines_197;
my @grep_with_filename_197;
for my $i (0..@grep_lines_197-1) {
    if (scalar grep { $_ eq $grep_lines_197[$i] } @grep_filtered_197) {
        push @grep_with_filename_197, $grep_filenames_197[$i] . ':' . $grep_lines_197[$i];
    }
}
$grep_result_197 = join "\n", @grep_with_filename_197;
if (!($grep_result_197 =~ m{\n\z}msx || $grep_result_197 eq q{})) {
    $grep_result_197 .= "\n";
}
print $grep_result_197;
$CHILD_ERROR = scalar @grep_filtered_197 > 0 ? 0 : 1;
print 'Result' . q{ } . '2...' . "\n";
$CHILD_ERROR = 0;
# Original bash: grep -l "pattern" *.txt | sort
{
    my $output_198 = q{};
    my $output_printed_198;
    my $pipeline_success_198 = 1;
        my $grep_result_198_0;
    my @grep_lines_198_0 = ();
    my @grep_filenames_198_0 = ();
    my @glob_files_198_0 = glob('*.txt');
    for my $glob_file (@glob_files_198_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_198_0, $line;
    push @grep_filenames_198_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_198_0 = grep { /pattern/msx } @grep_lines_198_0;
    my @matching_files_198_0;
    my %file_has_match_198_0;
    for my $i (0..@grep_lines_198_0-1) {
    if (scalar grep { $_ eq $grep_lines_198_0[$i] } @grep_filtered_198_0) {
    $file_has_match_198_0{$grep_filenames_198_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_198_0) {
    push @matching_files_198_0, $file;
    }
    $grep_result_198_0 = join "\n", @matching_files_198_0;
    $CHILD_ERROR = scalar @grep_filtered_198_0 > 0 ? 0 : 1;
    $output_198 = $grep_result_198_0;
    $output_198 = $grep_result_198_0;

        my @sort_lines_198_1 = split /\n/msx, $output_198;
    my @sort_sorted_198_1 = sort @sort_lines_198_1;
    my $output_198_1 = join "\n", @sort_sorted_198_1;
    if ($output_198_1 ne q{} && !($output_198_1 =~ m{\n\z}msx)) {
    $output_198_1 .= "\n";
    }
    $output_198 = $output_198_1;
    $output_198 = $output_198_1;
    if ($output_198 ne q{} && !defined $output_printed_198) {
        print $output_198;
        if (!($output_198 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_198 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_199;
my @grep_lines_199 = ();
my @grep_filenames_199 = ();
my @glob_files_199 = glob('*.txt');
for my $glob_file (@glob_files_199) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_199, $line;
            push @grep_filenames_199, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_199 = grep { /pattern/msx } @grep_lines_199;
my @non_matching_files_199;
my %file_has_match_199;
my %all_files_199;
my @all_glob_files_199 = glob('*.txt');
for my $file (@all_glob_files_199) {
    if (-f $file) {
        $all_files_199{$file} = 1;
    }
}
for my $i (0..@grep_lines_199-1) {
    if (scalar grep { $_ eq $grep_lines_199[$i] } @grep_filtered_199) {
        $file_has_match_199{$grep_filenames_199[$i]} = 1;
    }
}
for my $file (sort keys %all_files_199) {
    if (!exists $file_has_match_199{$file}) {
        push @non_matching_files_199, $file;
    }
}
$grep_result_199 = join "\n", @non_matching_files_199;
print $grep_result_199;
print "\n";
$CHILD_ERROR = $grep_result_199 ne q{} ? 0 : 1;
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

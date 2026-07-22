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
    my $output_193 = q{};
    my $output_printed_193;
    my $pipeline_success_193 = 1;
    $output_193 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_193 =~ m{\n\z}msx) ) { $output_193 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_193_1;
    my @grep_lines_193_1 = split /\n/msx, $output_193;
    my @grep_filtered_193_1 = grep { /TARGET/msx } @grep_lines_193_1;
    my @grep_with_context_193_1;
    for my $i (0..@grep_lines_193_1-1) {
    if (scalar grep { $_ eq $grep_lines_193_1[$i] } @grep_filtered_193_1) {
    push @grep_with_context_193_1, $grep_lines_193_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_193_1, $grep_lines_193_1[$j];
    }
    }
    }
    $grep_result_193_1 = join "\n", @grep_with_context_193_1;
    $CHILD_ERROR = scalar @grep_filtered_193_1 > 0 ? 0 : 1;
    $output_193 = $grep_result_193_1;
    $output_193 = $grep_result_193_1;
    if ((scalar @grep_filtered_193_1) == 0) {
        $pipeline_success_193 = 0;
    }
    if ($output_193 ne q{} && !defined $output_printed_193) {
        print $output_193;
        if (!($output_193 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_193 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
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
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_194_1, $grep_lines_194_1[$j];
    }
    }
    push @grep_with_context_194_1, $grep_lines_194_1[$i];
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
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
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_195_1, $grep_lines_195_1[$j];
    }
    }
    push @grep_with_context_195_1, $grep_lines_195_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_195_1, $grep_lines_195_1[$j];
    }
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
my $grep_result_196;
my @grep_lines_196 = ();
my @grep_filenames_196 = ();
my $find_files_recursive_196;
$find_files_recursive_196 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_196->($path, $pattern));
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
my @files_196 = $find_files_recursive_196->('.', '*.txt');
for my $file (@files_196) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_196, $line;
            push @grep_filenames_196, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_196 = grep { /pattern/msx } @grep_lines_196;
my @grep_with_filename_196;
for my $i (0..@grep_lines_196-1) {
    if (scalar grep { $_ eq $grep_lines_196[$i] } @grep_filtered_196) {
        push @grep_with_filename_196, $grep_filenames_196[$i] . ':' . $grep_lines_196[$i];
    }
}
$grep_result_196 = join "\n", @grep_with_filename_196;
if (!($grep_result_196 =~ m{\n\z}msx || $grep_result_196 eq q{})) {
    $grep_result_196 .= "\n";
}
print $grep_result_196;
$CHILD_ERROR = scalar @grep_filtered_196 > 0 ? 0 : 1;
print 'Result' . q{ } . '2...' . "\n";
$CHILD_ERROR = 0;
# Original bash: grep -l "pattern" *.txt | sort
{
    my $output_197 = q{};
    my $output_printed_197;
    my $pipeline_success_197 = 1;
        my $grep_result_197_0;
    my @grep_lines_197_0 = ();
    my @grep_filenames_197_0 = ();
    my @glob_files_197_0 = glob('*.txt');
    for my $glob_file (@glob_files_197_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_197_0, $line;
    push @grep_filenames_197_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_197_0 = grep { /pattern/msx } @grep_lines_197_0;
    my @matching_files_197_0;
    my %file_has_match_197_0;
    for my $i (0..@grep_lines_197_0-1) {
    if (scalar grep { $_ eq $grep_lines_197_0[$i] } @grep_filtered_197_0) {
    $file_has_match_197_0{$grep_filenames_197_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_197_0) {
    push @matching_files_197_0, $file;
    }
    $grep_result_197_0 = join "\n", @matching_files_197_0;
    $CHILD_ERROR = scalar @grep_filtered_197_0 > 0 ? 0 : 1;
    $output_197 = $grep_result_197_0;
    $output_197 = $grep_result_197_0;

        my @sort_lines_197_1 = split /\n/msx, $output_197;
    my @sort_sorted_197_1 = sort @sort_lines_197_1;
    my $output_197_1 = join "\n", @sort_sorted_197_1;
    if ($output_197_1 ne q{} && !($output_197_1 =~ m{\n\z}msx)) {
    $output_197_1 .= "\n";
    }
    $output_197 = $output_197_1;
    $output_197 = $output_197_1;
    if ($output_197 ne q{} && !defined $output_printed_197) {
        print $output_197;
        if (!($output_197 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_197 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_198;
my @grep_lines_198 = ();
my @grep_filenames_198 = ();
my @glob_files_198 = glob('*.txt');
for my $glob_file (@glob_files_198) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_198, $line;
            push @grep_filenames_198, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_198 = grep { /pattern/msx } @grep_lines_198;
my @non_matching_files_198;
my %file_has_match_198;
my %all_files_198;
my @all_glob_files_198 = glob('*.txt');
for my $file (@all_glob_files_198) {
    if (-f $file) {
        $all_files_198{$file} = 1;
    }
}
for my $i (0..@grep_lines_198-1) {
    if (scalar grep { $_ eq $grep_lines_198[$i] } @grep_filtered_198) {
        $file_has_match_198{$grep_filenames_198[$i]} = 1;
    }
}
for my $file (sort keys %all_files_198) {
    if (!exists $file_has_match_198{$file}) {
        push @non_matching_files_198, $file;
    }
}
$grep_result_198 = join "\n", @non_matching_files_198;
print $grep_result_198;
print "\n";
$CHILD_ERROR = $grep_result_198 ne q{} ? 0 : 1;
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

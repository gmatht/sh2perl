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
    my $output_189 = q{};
    my $output_printed_189;
    my $pipeline_success_189 = 1;
    $output_189 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_189 =~ m{\n\z}msx) ) { $output_189 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_189_1;
    my @grep_lines_189_1 = split /\n/msx, $output_189;
    my @grep_filtered_189_1 = grep { /TARGET/msx } @grep_lines_189_1;
    my @grep_with_context_189_1;
    for my $i (0..@grep_lines_189_1-1) {
    if (scalar grep { $_ eq $grep_lines_189_1[$i] } @grep_filtered_189_1) {
    push @grep_with_context_189_1, $grep_lines_189_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_189_1, $grep_lines_189_1[$j];
    }
    }
    }
    $grep_result_189_1 = join "\n", @grep_with_context_189_1;
    $CHILD_ERROR = scalar @grep_filtered_189_1 > 0 ? 0 : 1;
    $output_189 = $grep_result_189_1;
    $output_189 = $grep_result_189_1;
    if ((scalar @grep_filtered_189_1) == 0) {
        $pipeline_success_189 = 0;
    }
    if ($output_189 ne q{} && !defined $output_printed_189) {
        print $output_189;
        if (!($output_189 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_189 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_190 = q{};
    my $output_printed_190;
    my $pipeline_success_190 = 1;
    $output_190 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_190 =~ m{\n\z}msx) ) { $output_190 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_190_1;
    my @grep_lines_190_1 = split /\n/msx, $output_190;
    my @grep_filtered_190_1 = grep { /TARGET/msx } @grep_lines_190_1;
    my @grep_with_context_190_1;
    for my $i (0..@grep_lines_190_1-1) {
    if (scalar grep { $_ eq $grep_lines_190_1[$i] } @grep_filtered_190_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_190_1, $grep_lines_190_1[$j];
    }
    }
    push @grep_with_context_190_1, $grep_lines_190_1[$i];
    }
    }
    $grep_result_190_1 = join "\n", @grep_with_context_190_1;
    $CHILD_ERROR = scalar @grep_filtered_190_1 > 0 ? 0 : 1;
    $output_190 = $grep_result_190_1;
    $output_190 = $grep_result_190_1;
    if ((scalar @grep_filtered_190_1) == 0) {
        $pipeline_success_190 = 0;
    }
    if ($output_190 ne q{} && !defined $output_printed_190) {
        print $output_190;
        if (!($output_190 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_190 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_191 = q{};
    my $output_printed_191;
    my $pipeline_success_191 = 1;
    $output_191 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_191 =~ m{\n\z}msx) ) { $output_191 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_191_1;
    my @grep_lines_191_1 = split /\n/msx, $output_191;
    my @grep_filtered_191_1 = grep { /TARGET/msx } @grep_lines_191_1;
    my @grep_with_context_191_1;
    for my $i (0..@grep_lines_191_1-1) {
    if (scalar grep { $_ eq $grep_lines_191_1[$i] } @grep_filtered_191_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_191_1, $grep_lines_191_1[$j];
    }
    }
    push @grep_with_context_191_1, $grep_lines_191_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_191_1, $grep_lines_191_1[$j];
    }
    }
    }
    $grep_result_191_1 = join "\n", @grep_with_context_191_1;
    $CHILD_ERROR = scalar @grep_filtered_191_1 > 0 ? 0 : 1;
    $output_191 = $grep_result_191_1;
    $output_191 = $grep_result_191_1;
    if ((scalar @grep_filtered_191_1) == 0) {
        $pipeline_success_191 = 0;
    }
    if ($output_191 ne q{} && !defined $output_printed_191) {
        print $output_191;
        if (!($output_191 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_191 ) { $main_exit_code = 1; }
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
my $grep_result_192;
my @grep_lines_192 = ();
my @grep_filenames_192 = ();
sub find_files_recursive_192 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_192($path, $pattern));
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
my @files_192 = find_files_recursive_192('.', '*.txt');
for my $file (@files_192) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_192, $line;
            push @grep_filenames_192, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_192 = grep { /pattern/msx } @grep_lines_192;
my @grep_with_filename_192;
for my $i (0..@grep_lines_192-1) {
    if (scalar grep { $_ eq $grep_lines_192[$i] } @grep_filtered_192) {
        push @grep_with_filename_192, "$grep_filenames_192[$i]:$grep_lines_192[$i]";
    }
}
$grep_result_192 = join "\n", @grep_with_filename_192;
if (!($grep_result_192 =~ m{\n\z}msx || $grep_result_192 eq q{})) {
    $grep_result_192 .= "\n";
}
print $grep_result_192;
$CHILD_ERROR = scalar @grep_filtered_192 > 0 ? 0 : 1;
print 'Result' . q{ } . '2...' . "\n";
$CHILD_ERROR = 0;
# Original bash: grep -l "pattern" *.txt | sort
{
    my $output_193 = q{};
    my $output_printed_193;
    my $pipeline_success_193 = 1;
        my $grep_result_193_0;
    my @grep_lines_193_0 = ();
    my @grep_filenames_193_0 = ();
    my @glob_files_193_0 = glob('*.txt');
    for my $glob_file (@glob_files_193_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_193_0, $line;
    push @grep_filenames_193_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_193_0 = grep { /pattern/msx } @grep_lines_193_0;
    my @matching_files_193_0;
    my %file_has_match_193_0;
    for my $i (0..@grep_lines_193_0-1) {
    if (scalar grep { $_ eq $grep_lines_193_0[$i] } @grep_filtered_193_0) {
    $file_has_match_193_0{$grep_filenames_193_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_193_0) {
    push @matching_files_193_0, $file;
    }
    $grep_result_193_0 = join "\n", @matching_files_193_0;
    $CHILD_ERROR = scalar @grep_filtered_193_0 > 0 ? 0 : 1;
    $output_193 = $grep_result_193_0;
    $output_193 = $grep_result_193_0;
    if ((scalar @grep_filtered_193_0) == 0) {
        $pipeline_success_193 = 0;
    }

        my @sort_lines_193_1 = split /\n/msx, $output_193;
    my @sort_sorted_193_1 = sort @sort_lines_193_1;
    my $output_193_1 = join "\n", @sort_sorted_193_1;
    if ($output_193_1 ne q{} && !($output_193_1 =~ m{\n\z}msx)) {
    $output_193_1 .= "\n";
    }
    $output_193 = $output_193_1;
    $output_193 = $output_193_1;
    if ($output_193 ne q{} && !defined $output_printed_193) {
        print $output_193;
        if (!($output_193 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_193 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_194;
my @grep_lines_194 = ();
my @grep_filenames_194 = ();
my @glob_files_194 = glob('*.txt');
for my $glob_file (@glob_files_194) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_194, $line;
            push @grep_filenames_194, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_194 = grep { /pattern/msx } @grep_lines_194;
my @non_matching_files_194;
my %file_has_match_194;
my %all_files_194;
my @all_glob_files_194 = glob('*.txt');
for my $file (@all_glob_files_194) {
    if (-f $file) {
        $all_files_194{$file} = 1;
    }
}
for my $i (0..@grep_lines_194-1) {
    if (scalar grep { $_ eq $grep_lines_194[$i] } @grep_filtered_194) {
        $file_has_match_194{$grep_filenames_194[$i]} = 1;
    }
}
for my $file (sort keys %all_files_194) {
    if (!exists $file_has_match_194{$file}) {
        push @non_matching_files_194, $file;
    }
}
$grep_result_194 = join "\n", @non_matching_files_194;
print $grep_result_194;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_194 > 0 ? 0 : 1;
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

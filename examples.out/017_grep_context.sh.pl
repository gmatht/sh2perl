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
our $CHILD_ERROR;

$PROGRAM_NAME = '017_grep_context.sh';
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_172 = q{};
    my $output_printed_172;
    my $pipeline_success_172 = 1;
    $output_172 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_172 =~ m{\n\z}msx) ) { $output_172 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_172_1;
    my @grep_lines_172_1 = split /\n/msx, $output_172;
    my @grep_filtered_172_1 = grep { /TARGET/msx } @grep_lines_172_1;
    my @grep_with_context_172_1;
    for my $i (0..@grep_lines_172_1-1) {
    if (scalar grep { $_ eq $grep_lines_172_1[$i] } @grep_filtered_172_1) {
    push @grep_with_context_172_1, $grep_lines_172_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_172_1, $grep_lines_172_1[$j];
    }
    }
    }
    $grep_result_172_1 = join "\n", @grep_with_context_172_1;
    $CHILD_ERROR = scalar @grep_filtered_172_1 > 0 ? 0 : 1;
    $output_172 = $grep_result_172_1;
    $output_172 = $grep_result_172_1;
    if ((scalar @grep_filtered_172_1) == 0) {
        $pipeline_success_172 = 0;
    }
    if ($output_172 ne q{} && !defined $output_printed_172) {
        print $output_172;
        if (!($output_172 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_172 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_173 = q{};
    my $output_printed_173;
    my $pipeline_success_173 = 1;
    $output_173 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_173 =~ m{\n\z}msx) ) { $output_173 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_173_1;
    my @grep_lines_173_1 = split /\n/msx, $output_173;
    my @grep_filtered_173_1 = grep { /TARGET/msx } @grep_lines_173_1;
    my @grep_with_context_173_1;
    for my $i (0..@grep_lines_173_1-1) {
    if (scalar grep { $_ eq $grep_lines_173_1[$i] } @grep_filtered_173_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_173_1, $grep_lines_173_1[$j];
    }
    }
    push @grep_with_context_173_1, $grep_lines_173_1[$i];
    }
    }
    $grep_result_173_1 = join "\n", @grep_with_context_173_1;
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_174 = q{};
    my $output_printed_174;
    my $pipeline_success_174 = 1;
    $output_174 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_174 =~ m{\n\z}msx) ) { $output_174 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_174_1;
    my @grep_lines_174_1 = split /\n/msx, $output_174;
    my @grep_filtered_174_1 = grep { /TARGET/msx } @grep_lines_174_1;
    my @grep_with_context_174_1;
    for my $i (0..@grep_lines_174_1-1) {
    if (scalar grep { $_ eq $grep_lines_174_1[$i] } @grep_filtered_174_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_174_1, $grep_lines_174_1[$j];
    }
    }
    push @grep_with_context_174_1, $grep_lines_174_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_174_1, $grep_lines_174_1[$j];
    }
    }
    }
    $grep_result_174_1 = join "\n", @grep_with_context_174_1;
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
my $grep_result_175;
my @grep_lines_175 = ();
my @grep_filenames_175 = ();
my $find_files_recursive_175;
$find_files_recursive_175 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_175->($path, $pattern));
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
my @files_175 = $find_files_recursive_175->('.', '*.txt');
for my $file (@files_175) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_175, $line;
            push @grep_filenames_175, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_175 = grep { /pattern/msx } @grep_lines_175;
my @grep_with_filename_175;
for my $i (0..@grep_lines_175-1) {
    if (scalar grep { $_ eq $grep_lines_175[$i] } @grep_filtered_175) {
        push @grep_with_filename_175, $grep_filenames_175[$i] . ':' . $grep_lines_175[$i];
    }
}
$grep_result_175 = join "\n", @grep_with_filename_175;
if (!($grep_result_175 =~ m{\n\z}msx || $grep_result_175 eq q{})) {
    $grep_result_175 .= "\n";
}
print $grep_result_175;
$CHILD_ERROR = scalar @grep_filtered_175 > 0 ? 0 : 1;
print 'Result' . q{ } . '2...' . "\n";
$CHILD_ERROR = 0;
# Original bash: grep -l "pattern" *.txt | sort
{
    my $output_176 = q{};
    my $output_printed_176;
    my $pipeline_success_176 = 1;
        my $grep_result_176_0;
    my @grep_lines_176_0 = ();
    my @grep_filenames_176_0 = ();
    my @glob_files_176_0 = glob('*.txt');
    for my $glob_file (@glob_files_176_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_176_0, $line;
    push @grep_filenames_176_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_176_0 = grep { /pattern/msx } @grep_lines_176_0;
    my @matching_files_176_0;
    my %file_has_match_176_0;
    for my $i (0..@grep_lines_176_0-1) {
    if (scalar grep { $_ eq $grep_lines_176_0[$i] } @grep_filtered_176_0) {
    $file_has_match_176_0{$grep_filenames_176_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_176_0) {
    push @matching_files_176_0, $file;
    }
    $grep_result_176_0 = join "\n", @matching_files_176_0;
    $CHILD_ERROR = scalar @grep_filtered_176_0 > 0 ? 0 : 1;
    $output_176 = $grep_result_176_0;
    $output_176 = $grep_result_176_0;

        my @sort_lines_176_1 = split /\n/msx, $output_176;
    my @sort_sorted_176_1 = sort @sort_lines_176_1;
    my $output_176_1 = join "\n", @sort_sorted_176_1;
    if ($output_176_1 ne q{} && !($output_176_1 =~ m{\n\z}msx)) {
    $output_176_1 .= "\n";
    }
    $output_176 = $output_176_1;
    $output_176 = $output_176_1;
    if ($output_176 ne q{} && !defined $output_printed_176) {
        print $output_176;
        if (!($output_176 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_176 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_177;
my @grep_lines_177 = ();
my @grep_filenames_177 = ();
my @glob_files_177 = glob('*.txt');
for my $glob_file (@glob_files_177) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_177, $line;
            push @grep_filenames_177, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_177 = grep { /pattern/msx } @grep_lines_177;
my @non_matching_files_177;
my %file_has_match_177;
my %all_files_177;
my @all_glob_files_177 = glob('*.txt');
for my $file (@all_glob_files_177) {
    if (-f $file) {
        $all_files_177{$file} = 1;
    }
}
for my $i (0..@grep_lines_177-1) {
    if (scalar grep { $_ eq $grep_lines_177[$i] } @grep_filtered_177) {
        $file_has_match_177{$grep_filenames_177[$i]} = 1;
    }
}
for my $file (sort keys %all_files_177) {
    if (!exists $file_has_match_177{$file}) {
        push @non_matching_files_177, $file;
    }
}
$grep_result_177 = join "\n", @non_matching_files_177;
print $grep_result_177;
print "\n";
$CHILD_ERROR = $grep_result_177 ne q{} ? 0 : 1;
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

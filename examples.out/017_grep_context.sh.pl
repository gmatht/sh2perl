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
    my $output_185 = q{};
    my $output_printed_185;
    my $pipeline_success_185 = 1;
    $output_185 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_185 =~ m{\n\z}msx) ) { $output_185 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_185_1;
    my @grep_lines_185_1 = split /\n/msx, $output_185;
    my @grep_filtered_185_1 = grep { /TARGET/msx } @grep_lines_185_1;
    my @grep_with_context_185_1;
    for my $i (0..@grep_lines_185_1-1) {
    if (scalar grep { $_ eq $grep_lines_185_1[$i] } @grep_filtered_185_1) {
    push @grep_with_context_185_1, $grep_lines_185_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_185_1, $grep_lines_185_1[$j];
    }
    }
    }
    $grep_result_185_1 = join "\n", @grep_with_context_185_1;
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_186 = q{};
    my $output_printed_186;
    my $pipeline_success_186 = 1;
    $output_186 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_186 =~ m{\n\z}msx) ) { $output_186 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_186_1;
    my @grep_lines_186_1 = split /\n/msx, $output_186;
    my @grep_filtered_186_1 = grep { /TARGET/msx } @grep_lines_186_1;
    my @grep_with_context_186_1;
    for my $i (0..@grep_lines_186_1-1) {
    if (scalar grep { $_ eq $grep_lines_186_1[$i] } @grep_filtered_186_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_186_1, $grep_lines_186_1[$j];
    }
    }
    push @grep_with_context_186_1, $grep_lines_186_1[$i];
    }
    }
    $grep_result_186_1 = join "\n", @grep_with_context_186_1;
    $CHILD_ERROR = scalar @grep_filtered_186_1 > 0 ? 0 : 1;
    $output_186 = $grep_result_186_1;
    $output_186 = $grep_result_186_1;
    if ((scalar @grep_filtered_186_1) == 0) {
        $pipeline_success_186 = 0;
    }
    if ($output_186 ne q{} && !defined $output_printed_186) {
        print $output_186;
        if (!($output_186 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_186 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_187 = q{};
    my $output_printed_187;
    my $pipeline_success_187 = 1;
    $output_187 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_187 =~ m{\n\z}msx) ) { $output_187 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_187_1;
    my @grep_lines_187_1 = split /\n/msx, $output_187;
    my @grep_filtered_187_1 = grep { /TARGET/msx } @grep_lines_187_1;
    my @grep_with_context_187_1;
    for my $i (0..@grep_lines_187_1-1) {
    if (scalar grep { $_ eq $grep_lines_187_1[$i] } @grep_filtered_187_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_187_1, $grep_lines_187_1[$j];
    }
    }
    push @grep_with_context_187_1, $grep_lines_187_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_187_1, $grep_lines_187_1[$j];
    }
    }
    }
    $grep_result_187_1 = join "\n", @grep_with_context_187_1;
    $CHILD_ERROR = scalar @grep_filtered_187_1 > 0 ? 0 : 1;
    $output_187 = $grep_result_187_1;
    $output_187 = $grep_result_187_1;
    if ((scalar @grep_filtered_187_1) == 0) {
        $pipeline_success_187 = 0;
    }
    if ($output_187 ne q{} && !defined $output_printed_187) {
        print $output_187;
        if (!($output_187 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_187 ) { $main_exit_code = 1; }
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
my $grep_result_188;
my @grep_lines_188 = ();
my @grep_filenames_188 = ();
my $find_files_recursive_188;
$find_files_recursive_188 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_188->($path, $pattern));
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
my @files_188 = $find_files_recursive_188->('.', '*.txt');
for my $file (@files_188) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_188, $line;
            push @grep_filenames_188, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_188 = grep { /pattern/msx } @grep_lines_188;
my @grep_with_filename_188;
for my $i (0..@grep_lines_188-1) {
    if (scalar grep { $_ eq $grep_lines_188[$i] } @grep_filtered_188) {
        push @grep_with_filename_188, $grep_filenames_188[$i] . ':' . $grep_lines_188[$i];
    }
}
$grep_result_188 = join "\n", @grep_with_filename_188;
if (!($grep_result_188 =~ m{\n\z}msx || $grep_result_188 eq q{})) {
    $grep_result_188 .= "\n";
}
print $grep_result_188;
$CHILD_ERROR = scalar @grep_filtered_188 > 0 ? 0 : 1;
print 'Result' . q{ } . '2...' . "\n";
$CHILD_ERROR = 0;
# Original bash: grep -l "pattern" *.txt | sort
{
    my $output_189 = q{};
    my $output_printed_189;
    my $pipeline_success_189 = 1;
        my $grep_result_189_0;
    my @grep_lines_189_0 = ();
    my @grep_filenames_189_0 = ();
    my @glob_files_189_0 = glob('*.txt');
    for my $glob_file (@glob_files_189_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_189_0, $line;
    push @grep_filenames_189_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_189_0 = grep { /pattern/msx } @grep_lines_189_0;
    my @matching_files_189_0;
    my %file_has_match_189_0;
    for my $i (0..@grep_lines_189_0-1) {
    if (scalar grep { $_ eq $grep_lines_189_0[$i] } @grep_filtered_189_0) {
    $file_has_match_189_0{$grep_filenames_189_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_189_0) {
    push @matching_files_189_0, $file;
    }
    $grep_result_189_0 = join "\n", @matching_files_189_0;
    $CHILD_ERROR = scalar @grep_filtered_189_0 > 0 ? 0 : 1;
    $output_189 = $grep_result_189_0;
    $output_189 = $grep_result_189_0;

        my @sort_lines_189_1 = split /\n/msx, $output_189;
    my @sort_sorted_189_1 = sort @sort_lines_189_1;
    my $output_189_1 = join "\n", @sort_sorted_189_1;
    if ($output_189_1 ne q{} && !($output_189_1 =~ m{\n\z}msx)) {
    $output_189_1 .= "\n";
    }
    $output_189 = $output_189_1;
    $output_189 = $output_189_1;
    if ($output_189 ne q{} && !defined $output_printed_189) {
        print $output_189;
        if (!($output_189 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_189 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_190;
my @grep_lines_190 = ();
my @grep_filenames_190 = ();
my @glob_files_190 = glob('*.txt');
for my $glob_file (@glob_files_190) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_190, $line;
            push @grep_filenames_190, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_190 = grep { /pattern/msx } @grep_lines_190;
my @non_matching_files_190;
my %file_has_match_190;
my %all_files_190;
my @all_glob_files_190 = glob('*.txt');
for my $file (@all_glob_files_190) {
    if (-f $file) {
        $all_files_190{$file} = 1;
    }
}
for my $i (0..@grep_lines_190-1) {
    if (scalar grep { $_ eq $grep_lines_190[$i] } @grep_filtered_190) {
        $file_has_match_190{$grep_filenames_190[$i]} = 1;
    }
}
for my $file (sort keys %all_files_190) {
    if (!exists $file_has_match_190{$file}) {
        push @non_matching_files_190, $file;
    }
}
$grep_result_190 = join "\n", @non_matching_files_190;
print $grep_result_190;
print "\n";
$CHILD_ERROR = $grep_result_190 ne q{} ? 0 : 1;
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

#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '017_grep_context.sh';
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
do {
    my $output_182 = q{};
    my $output_printed_182;
    my $pipeline_success_182 = 1;
    $output_182 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_182 =~ m{\n\z}) ) { $output_182 .= "\n"; }

        my $grep_result_182_1;
    my @grep_lines_182_1 = split /\n/msx, $output_182;
    my @grep_filtered_182_1 = grep { /TARGET/msx } @grep_lines_182_1;
    my @grep_with_context_182_1;
    for my $i (0..@grep_lines_182_1-1) {
    if (scalar grep { $_ eq $grep_lines_182_1[$i] } @grep_filtered_182_1) {
    push @grep_with_context_182_1, $grep_lines_182_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_182_1, $grep_lines_182_1[$j];
    }
    }
    }
    $grep_result_182_1 = join "\n", @grep_with_context_182_1;
    $CHILD_ERROR = scalar @grep_filtered_182_1 > 0 ? 0 : 1;
    $output_182 = $grep_result_182_1;
    $output_182 = $grep_result_182_1;
    if ((scalar @grep_filtered_182_1) == 0) {
        $pipeline_success_182 = 0;
    }
    if ($output_182 ne q{} && !defined $output_printed_182) {
        print $output_182;
        if (!($output_182 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_182 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
do {
    my $output_183 = q{};
    my $output_printed_183;
    my $pipeline_success_183 = 1;
    $output_183 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_183 =~ m{\n\z}) ) { $output_183 .= "\n"; }

        my $grep_result_183_1;
    my @grep_lines_183_1 = split /\n/msx, $output_183;
    my @grep_filtered_183_1 = grep { /TARGET/msx } @grep_lines_183_1;
    my @grep_with_context_183_1;
    for my $i (0..@grep_lines_183_1-1) {
    if (scalar grep { $_ eq $grep_lines_183_1[$i] } @grep_filtered_183_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_183_1, $grep_lines_183_1[$j];
    }
    }
    push @grep_with_context_183_1, $grep_lines_183_1[$i];
    }
    }
    $grep_result_183_1 = join "\n", @grep_with_context_183_1;
    $CHILD_ERROR = scalar @grep_filtered_183_1 > 0 ? 0 : 1;
    $output_183 = $grep_result_183_1;
    $output_183 = $grep_result_183_1;
    if ((scalar @grep_filtered_183_1) == 0) {
        $pipeline_success_183 = 0;
    }
    if ($output_183 ne q{} && !defined $output_printed_183) {
        print $output_183;
        if (!($output_183 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_183 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
do {
    my $output_184 = q{};
    my $output_printed_184;
    my $pipeline_success_184 = 1;
    $output_184 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_184 =~ m{\n\z}) ) { $output_184 .= "\n"; }

        my $grep_result_184_1;
    my @grep_lines_184_1 = split /\n/msx, $output_184;
    my @grep_filtered_184_1 = grep { /TARGET/msx } @grep_lines_184_1;
    my @grep_with_context_184_1;
    for my $i (0..@grep_lines_184_1-1) {
    if (scalar grep { $_ eq $grep_lines_184_1[$i] } @grep_filtered_184_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_184_1, $grep_lines_184_1[$j];
    }
    }
    push @grep_with_context_184_1, $grep_lines_184_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_184_1, $grep_lines_184_1[$j];
    }
    }
    }
    $grep_result_184_1 = join "\n", @grep_with_context_184_1;
    $CHILD_ERROR = scalar @grep_filtered_184_1 > 0 ? 0 : 1;
    $output_184 = $grep_result_184_1;
    $output_184 = $grep_result_184_1;
    if ((scalar @grep_filtered_184_1) == 0) {
        $pipeline_success_184 = 0;
    }
    if ($output_184 ne q{} && !defined $output_printed_184) {
        print $output_184;
        if (!($output_184 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_184 ) { $main_exit_code = 1; }
    }
say "Creating test files...";
open my $fh, '>', 'temp_file1.txt' or die "temp_file1.txt: $!\n";
say {*fh} "pattern in file1";
close $fh;
open my $fh, '>', 'temp_file2.txt' or die "temp_file2.txt: $!\n";
say {*fh} "no pattern in file2";
close $fh;
open my $fh, '>', 'temp_file3.txt' or die "temp_file3.txt: $!\n";
say {*fh} "pattern in file3";
close $fh;
say "Recursive search results:";
my $grep_result_185;
my @grep_lines_185 = ();
my @grep_filenames_185 = ();
my $find_files_recursive_185;
$find_files_recursive_185 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_185->($path, $pattern));
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
my @files_185 = $find_files_recursive_185->('.', '*.txt');
for my $file (@files_185) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_185, $line;
            push @grep_filenames_185, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_185 = grep { /pattern/msx } @grep_lines_185;
my @grep_with_filename_185;
for my $i (0..@grep_lines_185-1) {
    if (scalar grep { $_ eq $grep_lines_185[$i] } @grep_filtered_185) {
        push @grep_with_filename_185, $grep_filenames_185[$i] . ':' . $grep_lines_185[$i];
    }
}
$grep_result_185 = join "\n", @grep_with_filename_185;
if (!($grep_result_185 =~ m{\n\z} || $grep_result_185 eq q{})) {
    $grep_result_185 .= "\n";
}
print $grep_result_185;
$CHILD_ERROR = scalar @grep_filtered_185 > 0 ? 0 : 1;
say 'Result' . q{ } . '2...';
# Original bash: grep -l "pattern" *.txt | sort
do {
    my $output_186 = q{};
    my $output_printed_186;
    my $pipeline_success_186 = 1;
        my $grep_result_186_0;
    my @grep_lines_186_0 = ();
    my @grep_filenames_186_0 = ();
    my @glob_files_186_0 = glob('*.txt');
    for my $glob_file (@glob_files_186_0) {
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_186_0, $line;
    push @grep_filenames_186_0, $glob_file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_186_0 = grep { /pattern/msx } @grep_lines_186_0;
    my @matching_files_186_0;
    my %file_has_match_186_0;
    for my $i (0..@grep_lines_186_0-1) {
    if (scalar grep { $_ eq $grep_lines_186_0[$i] } @grep_filtered_186_0) {
    $file_has_match_186_0{$grep_filenames_186_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_186_0) {
    push @matching_files_186_0, $file;
    }
    $grep_result_186_0 = join "\n", @matching_files_186_0;
    $CHILD_ERROR = scalar @grep_filtered_186_0 > 0 ? 0 : 1;
    $output_186 = $grep_result_186_0;
    $output_186 = $grep_result_186_0;

        my @sort_lines_186_1 = split /\n/, $output_186;
    my @sort_sorted_186_1 = sort @sort_lines_186_1;
    my $output_186_1 = join("\n", @sort_sorted_186_1);
    $output_186 = $output_186_1;
    $output_186 = $output_186_1;
    if ($output_186 ne q{} && !defined $output_printed_186) {
        print $output_186;
        if (!($output_186 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_186 ) { $main_exit_code = 1; }
    }
say 'Result' . q{ } . '3...';
my $grep_result_187;
my @grep_lines_187 = ();
my @grep_filenames_187 = ();
my @glob_files_187 = glob('*.txt');
for my $glob_file (@glob_files_187) {
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_187, $line;
            push @grep_filenames_187, $glob_file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_187 = grep { /pattern/msx } @grep_lines_187;
my @non_matching_files_187;
my %file_has_match_187;
my %all_files_187;
my @all_glob_files_187 = glob('*.txt');
for my $file (@all_glob_files_187) {
    if (-f $file) {
        $all_files_187{$file} = 1;
    }
}
for my $i (0..@grep_lines_187-1) {
    if (scalar grep { $_ eq $grep_lines_187[$i] } @grep_filtered_187) {
        $file_has_match_187{$grep_filenames_187[$i]} = 1;
    }
}
for my $file (sort keys %all_files_187) {
    if (!exists $file_has_match_187{$file}) {
        push @non_matching_files_187, $file;
    }
}
$grep_result_187 = join "\n", @non_matching_files_187;
print $grep_result_187;
print "\n";
$CHILD_ERROR = $grep_result_187 ne q{} ? 0 : 1;
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

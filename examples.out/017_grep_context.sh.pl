#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
my $output         = q{};
our $CHILD_ERROR;

# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
my $output_0 = qx{command echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -A 2 TARGET};
chomp $output_0;
print $output_0, "\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
my $output_1 = qx{command echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -B 2 TARGET};
chomp $output_1;
print $output_1, "\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
my $output_2 = qx{command echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -C 1 TARGET};
chomp $output_2;
print $output_2, "\n";
print "Creating test files...\n";
open my $fh, '>', 'temp_file1.txt' or die "temp_file1.txt: $!\n";
print {*fh} "pattern in file1", "\n";
close $fh;
open my $fh, '>', 'temp_file2.txt' or die "temp_file2.txt: $!\n";
print {*fh} "no pattern in file2", "\n";
close $fh;
open my $fh, '>', 'temp_file3.txt' or die "temp_file3.txt: $!\n";
print {*fh} "pattern in file3", "\n";
close $fh;
print "Recursive search results:\n";
my $grep_result_3;
my @grep_lines_3 = ();
my @grep_filenames_3 = ();
my $find_files_recursive_3;
$find_files_recursive_3 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_3->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/ms) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_3 = $find_files_recursive_3->('.', '*.txt');
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
my @grep_filtered_3 = grep { {pattern} } @grep_lines_3;
my @grep_with_filename_3;
for my $i (0..@grep_lines_3-1) {
    if (scalar grep { $_ eq $grep_lines_3[$i] } @grep_filtered_3) {
        push @grep_with_filename_3, $grep_filenames_3[$i] . ':' . $grep_lines_3[$i];
    }
}
$grep_result_3 = join "\n", @grep_with_filename_3;
if (!($grep_result_3 =~ m{\n\z} || $grep_result_3 eq q{})) {
    $grep_result_3 .= "\n";
}
print $grep_result_3;
$CHILD_ERROR = scalar @grep_filtered_3 > 0 ? 0 : 1;
print "Result' . q{ } . '2...\
";
# Original bash: grep -l "pattern" *.txt | sort
my $output_4 = qx{command grep -l pattern '*.txt' | sort};
chomp $output_4;
print $output_4, "\n";
print "Result' . q{ } . '3...\
";
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
my @grep_filtered_5 = grep { {pattern} } @grep_lines_5;
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
$CHILD_ERROR = $grep_result_5 ne q{} ? 0 : 1;
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
my $matched = do { my $input_data = "test"; my $grep_result_6;
my @grep_lines_6 = split /\n/msx, $input_data;
my @grep_filtered_6 = grep { /.*/s } @grep_lines_6;
$grep_result_6 = scalar @grep_filtered_6 . "\n";
$CHILD_ERROR = scalar @grep_filtered_6 > 0 ? 0 : 1;
 };
print "  grep_exit: ${\($? >> 8)}\n";
print "  match_count: $matched\n";


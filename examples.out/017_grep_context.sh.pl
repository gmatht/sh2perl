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

# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
<<<<<<< HEAD
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
    push @grep_with_context_187_1, $grep_lines_187_1[$i];
    for my $j (($i + 1)..($i + 2)) {
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_188 = q{};
    my $output_printed_188;
    my $pipeline_success_188 = 1;
    $output_188 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_188 =~ m{\n\z}msx) ) { $output_188 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_188_1;
    my @grep_lines_188_1 = split /\n/msx, $output_188;
    my @grep_filtered_188_1 = grep { /TARGET/msx } @grep_lines_188_1;
    my @grep_with_context_188_1;
    for my $i (0..@grep_lines_188_1-1) {
    if (scalar grep { $_ eq $grep_lines_188_1[$i] } @grep_filtered_188_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_188_1, $grep_lines_188_1[$j];
    }
    }
    push @grep_with_context_188_1, $grep_lines_188_1[$i];
    }
    }
    $grep_result_188_1 = join "\n", @grep_with_context_188_1;
    $CHILD_ERROR = scalar @grep_filtered_188_1 > 0 ? 0 : 1;
    $output_188 = $grep_result_188_1;
    $output_188 = $grep_result_188_1;
    if ((scalar @grep_filtered_188_1) == 0) {
        $pipeline_success_188 = 0;
    }
    if ($output_188 ne q{} && !defined $output_printed_188) {
        print $output_188;
        if (!($output_188 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_188 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
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
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_189_1, $grep_lines_189_1[$j];
    }
    }
    push @grep_with_context_189_1, $grep_lines_189_1[$i];
    for my $j (($i + 1)..($i + 1)) {
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
=======
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
    push @grep_with_context_186_1, $grep_lines_186_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_186_1, $grep_lines_186_1[$j];
    }
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
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
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_187_1, $grep_lines_187_1[$j];
    }
    }
    push @grep_with_context_187_1, $grep_lines_187_1[$i];
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_188 = q{};
    my $output_printed_188;
    my $pipeline_success_188 = 1;
    $output_188 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_188 =~ m{\n\z}msx) ) { $output_188 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_188_1;
    my @grep_lines_188_1 = split /\n/msx, $output_188;
    my @grep_filtered_188_1 = grep { /TARGET/msx } @grep_lines_188_1;
    my @grep_with_context_188_1;
    for my $i (0..@grep_lines_188_1-1) {
    if (scalar grep { $_ eq $grep_lines_188_1[$i] } @grep_filtered_188_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_188_1, $grep_lines_188_1[$j];
    }
    }
    push @grep_with_context_188_1, $grep_lines_188_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_188_1, $grep_lines_188_1[$j];
    }
    }
    }
    $grep_result_188_1 = join "\n", @grep_with_context_188_1;
    $CHILD_ERROR = scalar @grep_filtered_188_1 > 0 ? 0 : 1;
    $output_188 = $grep_result_188_1;
    $output_188 = $grep_result_188_1;
    if ((scalar @grep_filtered_188_1) == 0) {
        $pipeline_success_188 = 0;
    }
    if ($output_188 ne q{} && !defined $output_printed_188) {
        print $output_188;
        if (!($output_188 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_188 ) { $main_exit_code = 1; }
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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
<<<<<<< HEAD
my $grep_result_190;
my @grep_lines_190 = ();
my @grep_filenames_190 = ();
my $find_files_recursive_190;
$find_files_recursive_190 = sub {
=======
my $grep_result_189;
my @grep_lines_189 = ();
my @grep_filenames_189 = ();
sub find_files_recursive_189 {
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
<<<<<<< HEAD
                @files = (@files, $find_files_recursive_190->($path, $pattern));
=======
                @files = (@files, find_files_recursive_189($path, $pattern));
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
<<<<<<< HEAD
};
my @files_190 = $find_files_recursive_190->('.', '*.txt');
for my $file (@files_190) {
=======
}
my @files_189 = find_files_recursive_189('.', '*.txt');
for my $file (@files_189) {
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
<<<<<<< HEAD
            push @grep_lines_190, $line;
            push @grep_filenames_190, $file;
=======
            push @grep_lines_189, $line;
            push @grep_filenames_189, $file;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
<<<<<<< HEAD
my @grep_filtered_190 = grep { /pattern/msx } @grep_lines_190;
my @grep_with_filename_190;
for my $i (0..@grep_lines_190-1) {
    if (scalar grep { $_ eq $grep_lines_190[$i] } @grep_filtered_190) {
        push @grep_with_filename_190, $grep_filenames_190[$i] . ':' . $grep_lines_190[$i];
    }
}
$grep_result_190 = join "\n", @grep_with_filename_190;
if (!($grep_result_190 =~ m{\n\z}msx || $grep_result_190 eq q{})) {
    $grep_result_190 .= "\n";
}
print $grep_result_190;
$CHILD_ERROR = scalar @grep_filtered_190 > 0 ? 0 : 1;
=======
my @grep_filtered_189 = grep { /pattern/msx } @grep_lines_189;
my @grep_with_filename_189;
for my $i (0..@grep_lines_189-1) {
    if (scalar grep { $_ eq $grep_lines_189[$i] } @grep_filtered_189) {
        push @grep_with_filename_189, "$grep_filenames_189[$i]:$grep_lines_189[$i]";
    }
}
$grep_result_189 = join "\n", @grep_with_filename_189;
if (!($grep_result_189 =~ m{\n\z}msx || $grep_result_189 eq q{})) {
    $grep_result_189 .= "\n";
}
print $grep_result_189;
$CHILD_ERROR = scalar @grep_filtered_189 > 0 ? 0 : 1;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
print 'Result' . q{ } . '2...' . "\n";
$CHILD_ERROR = 0;
# Original bash: grep -l "pattern" *.txt | sort
{
<<<<<<< HEAD
    my $output_191 = q{};
    my $output_printed_191;
    my $pipeline_success_191 = 1;
        my $grep_result_191_0;
    my @grep_lines_191_0 = ();
    my @grep_filenames_191_0 = ();
    my @glob_files_191_0 = glob('*.txt');
    for my $glob_file (@glob_files_191_0) {
=======
    my $output_190 = q{};
    my $output_printed_190;
    my $pipeline_success_190 = 1;
        my $grep_result_190_0;
    my @grep_lines_190_0 = ();
    my @grep_filenames_190_0 = ();
    my @glob_files_190_0 = glob('*.txt');
    for my $glob_file (@glob_files_190_0) {
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    if (-f $glob_file) {
    open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
<<<<<<< HEAD
    push @grep_lines_191_0, $line;
    push @grep_filenames_191_0, $glob_file;
=======
    push @grep_lines_190_0, $line;
    push @grep_filenames_190_0, $glob_file;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
<<<<<<< HEAD
    my @grep_filtered_191_0 = grep { /pattern/msx } @grep_lines_191_0;
    my @matching_files_191_0;
    my %file_has_match_191_0;
    for my $i (0..@grep_lines_191_0-1) {
    if (scalar grep { $_ eq $grep_lines_191_0[$i] } @grep_filtered_191_0) {
    $file_has_match_191_0{$grep_filenames_191_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_191_0) {
    push @matching_files_191_0, $file;
=======
    my @grep_filtered_190_0 = grep { /pattern/msx } @grep_lines_190_0;
    my @matching_files_190_0;
    my %file_has_match_190_0;
    for my $i (0..@grep_lines_190_0-1) {
    if (scalar grep { $_ eq $grep_lines_190_0[$i] } @grep_filtered_190_0) {
    $file_has_match_190_0{$grep_filenames_190_0[$i]} = 1;
    }
    }
    for my $file (sort keys %file_has_match_190_0) {
    push @matching_files_190_0, $file;
    }
    $grep_result_190_0 = join "\n", @matching_files_190_0;
    $CHILD_ERROR = scalar @grep_filtered_190_0 > 0 ? 0 : 1;
    $output_190 = $grep_result_190_0;
    $output_190 = $grep_result_190_0;
    if ((scalar @grep_filtered_190_0) == 0) {
        $pipeline_success_190 = 0;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    }
    $grep_result_191_0 = join "\n", @matching_files_191_0;
    $CHILD_ERROR = scalar @grep_filtered_191_0 > 0 ? 0 : 1;
    $output_191 = $grep_result_191_0;
    $output_191 = $grep_result_191_0;

<<<<<<< HEAD
        my @sort_lines_191_1 = split /\n/msx, $output_191;
    my @sort_sorted_191_1 = sort @sort_lines_191_1;
    my $output_191_1 = join "\n", @sort_sorted_191_1;
    if ($output_191_1 ne q{} && !($output_191_1 =~ m{\n\z}msx)) {
    $output_191_1 .= "\n";
    }
    $output_191 = $output_191_1;
    $output_191 = $output_191_1;
    if ($output_191 ne q{} && !defined $output_printed_191) {
        print $output_191;
        if (!($output_191 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_191 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_192;
my @grep_lines_192 = ();
my @grep_filenames_192 = ();
my @glob_files_192 = glob('*.txt');
for my $glob_file (@glob_files_192) {
=======
        my @sort_lines_190_1 = split /\n/msx, $output_190;
    my @sort_sorted_190_1 = sort @sort_lines_190_1;
    my $output_190_1 = join "\n", @sort_sorted_190_1;
    if ($output_190_1 ne q{} && !($output_190_1 =~ m{\n\z}msx)) {
    $output_190_1 .= "\n";
    }
    $output_190 = $output_190_1;
    $output_190 = $output_190_1;
    if ($output_190 ne q{} && !defined $output_printed_190) {
        print $output_190;
        if (!($output_190 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_190 ) { $main_exit_code = 1; }
    }
print 'Result' . q{ } . '3...' . "\n";
$CHILD_ERROR = 0;
my $grep_result_191;
my @grep_lines_191 = ();
my @grep_filenames_191 = ();
my @glob_files_191 = glob('*.txt');
for my $glob_file (@glob_files_191) {
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
    if (-f $glob_file) {
        open my $fh, '<', $glob_file or die "Cannot open $glob_file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
<<<<<<< HEAD
            push @grep_lines_192, $line;
            push @grep_filenames_192, $glob_file;
=======
            push @grep_lines_191, $line;
            push @grep_filenames_191, $glob_file;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
<<<<<<< HEAD
my @grep_filtered_192 = grep { /pattern/msx } @grep_lines_192;
my @non_matching_files_192;
my %file_has_match_192;
my %all_files_192;
my @all_glob_files_192 = glob('*.txt');
for my $file (@all_glob_files_192) {
    if (-f $file) {
        $all_files_192{$file} = 1;
    }
}
for my $i (0..@grep_lines_192-1) {
    if (scalar grep { $_ eq $grep_lines_192[$i] } @grep_filtered_192) {
        $file_has_match_192{$grep_filenames_192[$i]} = 1;
    }
}
for my $file (sort keys %all_files_192) {
    if (!exists $file_has_match_192{$file}) {
        push @non_matching_files_192, $file;
    }
}
$grep_result_192 = join "\n", @non_matching_files_192;
print $grep_result_192;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_192 > 0 ? 0 : 1;
=======
my @grep_filtered_191 = grep { /pattern/msx } @grep_lines_191;
my @non_matching_files_191;
my %file_has_match_191;
my %all_files_191;
my @all_glob_files_191 = glob('*.txt');
for my $file (@all_glob_files_191) {
    if (-f $file) {
        $all_files_191{$file} = 1;
    }
}
for my $i (0..@grep_lines_191-1) {
    if (scalar grep { $_ eq $grep_lines_191[$i] } @grep_filtered_191) {
        $file_has_match_191{$grep_filenames_191[$i]} = 1;
    }
}
for my $file (sort keys %all_files_191) {
    if (!exists $file_has_match_191{$file}) {
        push @non_matching_files_191, $file;
    }
}
$grep_result_191 = join "\n", @non_matching_files_191;
print $grep_result_191;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_191 > 0 ? 0 : 1;
>>>>>>> aebd05460dfb3284730ab659345a8daedaeb6a9e
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

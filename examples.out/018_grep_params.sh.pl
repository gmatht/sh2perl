#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '018_grep_params.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== Basic grep parameters ==";
# Original bash: echo "text with pattern" | grep -i "PATTERN"
do {
    my $output_188 = q{};
    my $output_printed_188;
    my $pipeline_success_188 = 1;
    $output_188 .= 'text with pattern' . "\n";
if ( !($output_188 =~ m{\n\z}) ) { $output_188 .= "\n"; }

        my $grep_result_188_1;
    my @grep_lines_188_1 = split /\n/msx, $output_188;
    my @grep_filtered_188_1 = grep { /PATTERN/msxi } @grep_lines_188_1;
    $grep_result_188_1 = join "\n", @grep_filtered_188_1;
    if (!($grep_result_188_1 =~ m{\n\z} || $grep_result_188_1 eq q{})) {
    $grep_result_188_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_188_1 > 0 ? 0 : 1;
    $output_188 = $grep_result_188_1;
    $output_188 = $grep_result_188_1;
    if ((scalar @grep_filtered_188_1) == 0) {
        $pipeline_success_188 = 0;
    }
    if ($output_188 ne q{} && !defined $output_printed_188) {
        print $output_188;
        if (!($output_188 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_188 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
do {
    my $output_189 = q{};
    my $output_printed_189;
    my $pipeline_success_189 = 1;
    $output_189 .= "line1\nline2\nline3";
if ( !($output_189 =~ m{\n\z}) ) { $output_189 .= "\n"; }

        my $grep_result_189_1;
    my @grep_lines_189_1 = split /\n/msx, $output_189;
    my @grep_filtered_189_1 = grep { !/line2/msx } @grep_lines_189_1;
    $grep_result_189_1 = join "\n", @grep_filtered_189_1;
    if (!($grep_result_189_1 =~ m{\n\z} || $grep_result_189_1 eq q{})) {
    $grep_result_189_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_189_1 > 0 ? 0 : 1;
    $output_189 = $grep_result_189_1;
    $output_189 = $grep_result_189_1;
    if ((scalar @grep_filtered_189_1) == 0) {
        $pipeline_success_189 = 0;
    }
    if ($output_189 ne q{} && !defined $output_printed_189) {
        print $output_189;
        if (!($output_189 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_189 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
do {
    my $output_190 = q{};
    my $output_printed_190;
    my $pipeline_success_190 = 1;
    $output_190 .= "match\nno match\nmatch again";
if ( !($output_190 =~ m{\n\z}) ) { $output_190 .= "\n"; }

        my $grep_result_190_1;
    my @grep_lines_190_1 = split /\n/msx, $output_190;
    my @grep_filtered_190_1 = grep { /match/msx } @grep_lines_190_1;
    $grep_result_190_1 = scalar @grep_filtered_190_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_190_1 > 0 ? 0 : 1;
    $output_190 = $grep_result_190_1;
    $output_190 = $grep_result_190_1;
    if ((scalar @grep_filtered_190_1) == 0) {
        $pipeline_success_190 = 0;
    }
    if ($output_190 ne q{} && !defined $output_printed_190) {
        print $output_190;
        if (!($output_190 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_190 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
say "== Context parameters ==";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
do {
    my $output_191 = q{};
    my $output_printed_191;
    my $pipeline_success_191 = 1;
    $output_191 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_191 =~ m{\n\z}) ) { $output_191 .= "\n"; }

        my $grep_result_191_1;
    my @grep_lines_191_1 = split /\n/msx, $output_191;
    my @grep_filtered_191_1 = grep { /TARGET/msx } @grep_lines_191_1;
    my @grep_with_context_191_1;
    for my $i (0..@grep_lines_191_1-1) {
    if (scalar grep { $_ eq $grep_lines_191_1[$i] } @grep_filtered_191_1) {
    push @grep_with_context_191_1, $grep_lines_191_1[$i];
    for my $j (($i + 1)..($i + 2)) {
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
        if (!($output_191 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_191 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
do {
    my $output_192 = q{};
    my $output_printed_192;
    my $pipeline_success_192 = 1;
    $output_192 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_192 =~ m{\n\z}) ) { $output_192 .= "\n"; }

        my $grep_result_192_1;
    my @grep_lines_192_1 = split /\n/msx, $output_192;
    my @grep_filtered_192_1 = grep { /TARGET/msx } @grep_lines_192_1;
    my @grep_with_context_192_1;
    for my $i (0..@grep_lines_192_1-1) {
    if (scalar grep { $_ eq $grep_lines_192_1[$i] } @grep_filtered_192_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_192_1, $grep_lines_192_1[$j];
    }
    }
    push @grep_with_context_192_1, $grep_lines_192_1[$i];
    }
    }
    $grep_result_192_1 = join "\n", @grep_with_context_192_1;
    $CHILD_ERROR = scalar @grep_filtered_192_1 > 0 ? 0 : 1;
    $output_192 = $grep_result_192_1;
    $output_192 = $grep_result_192_1;
    if ((scalar @grep_filtered_192_1) == 0) {
        $pipeline_success_192 = 0;
    }
    if ($output_192 ne q{} && !defined $output_printed_192) {
        print $output_192;
        if (!($output_192 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_192 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
do {
    my $output_193 = q{};
    my $output_printed_193;
    my $pipeline_success_193 = 1;
    $output_193 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_193 =~ m{\n\z}) ) { $output_193 .= "\n"; }

        my $grep_result_193_1;
    my @grep_lines_193_1 = split /\n/msx, $output_193;
    my @grep_filtered_193_1 = grep { /TARGET/msx } @grep_lines_193_1;
    my @grep_with_context_193_1;
    for my $i (0..@grep_lines_193_1-1) {
    if (scalar grep { $_ eq $grep_lines_193_1[$i] } @grep_filtered_193_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_193_1, $grep_lines_193_1[$j];
    }
    }
    push @grep_with_context_193_1, $grep_lines_193_1[$i];
    for my $j (($i + 1)..($i + 1)) {
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
        if (!($output_193 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_193 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
say "== File handling parameters ==";
open my $fh, '>', 'temp_file.txt' or die "temp_file.txt: $!\n";
say {*fh} "content";
close $fh;
my $grep_result_194;
my @grep_lines_194 = ();
my @grep_filenames_194 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_194, $line;
        push @grep_filenames_194, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_194 = grep { /content/msx } @grep_lines_194;
my @grep_with_filename_194;
for my $line (@grep_filtered_194) {
    push @grep_with_filename_194, "temp_file.txt:$line";
}
$grep_result_194 = join "\n", @grep_with_filename_194;
if (!($grep_result_194 =~ m{\n\z} || $grep_result_194 eq q{})) {
    $grep_result_194 .= "\n";
}
print $grep_result_194;
$CHILD_ERROR = scalar @grep_filtered_194 > 0 ? 0 : 1;
my $grep_result_195;
my @grep_lines_195 = ();
my @grep_filenames_195 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_195, $line;
        push @grep_filenames_195, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_195 = grep { /content/msx } @grep_lines_195;
$grep_result_195 = join "\n", @grep_filtered_195;
if (!($grep_result_195 =~ m{\n\z} || $grep_result_195 eq q{})) {
    $grep_result_195 .= "\n";
}
print $grep_result_195;
$CHILD_ERROR = scalar @grep_filtered_195 > 0 ? 0 : 1;
my $grep_result_196;
my @grep_lines_196 = ();
my @grep_filenames_196 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_196, $line;
        push @grep_filenames_196, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_196 = grep { /content/msx } @grep_lines_196;
$grep_result_196 = @grep_filtered_196 > 0 ? "temp_file.txt" : "";
print $grep_result_196;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_196 > 0 ? 0 : 1;
my $grep_result_197;
my @grep_lines_197 = ();
my @grep_filenames_197 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_197, $line;
        push @grep_filenames_197, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_197 = grep { /nonexistent/msx } @grep_lines_197;
$grep_result_197 = @grep_filtered_197 == 0 ? "temp_file.txt" : "";
print $grep_result_197;
print "\n";
$CHILD_ERROR = $grep_result_197 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
say "== Output formatting parameters ==";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
do {
    my $output_199 = q{};
    my $output_printed_199;
    my $pipeline_success_199 = 1;
    $output_199 .= 'text with pattern in it' . "\n";
if ( !($output_199 =~ m{\n\z}) ) { $output_199 .= "\n"; }

        my $grep_result_199_1;
    my @grep_lines_199_1 = split /\n/msx, $output_199;
    my @grep_filtered_199_1 = grep { /pattern/msx } @grep_lines_199_1;
    my @grep_matches_199_1;
    foreach my $line (@grep_filtered_199_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_199_1, $1;
    }
    }
    $grep_result_199_1 = join "\n", @grep_matches_199_1;
    $CHILD_ERROR = scalar @grep_filtered_199_1 > 0 ? 0 : 1;
    $output_199 = $grep_result_199_1;
    $output_199 = $grep_result_199_1;
    if ((scalar @grep_filtered_199_1) == 0) {
        $pipeline_success_199 = 0;
    }
    if ($output_199 ne q{} && !defined $output_printed_199) {
        print $output_199;
        if (!($output_199 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_199 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
do {
    my $output_200 = q{};
    my $output_printed_200;
    my $pipeline_success_200 = 1;
    $output_200 .= 'text with pattern in it' . "\n";
if ( !($output_200 =~ m{\n\z}) ) { $output_200 .= "\n"; }

        my $grep_result_200_1;
    my @grep_lines_200_1 = split /\n/msx, $output_200;
    my @grep_filtered_200_1 = grep { /pattern/msx } @grep_lines_200_1;
    my @grep_with_offset_200_1;
    my $offset_200_1 = 0;
    for my $line (@grep_lines_200_1) {
    if (grep { $_ eq $line } @grep_filtered_200_1) {
    push @grep_with_offset_200_1, sprintf "%d:%s", $offset_200_1, $line;
    }
    $offset_200_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_200_1 = join "\n", @grep_with_offset_200_1;
    if (!($grep_result_200_1 =~ m{\n\z} || $grep_result_200_1 eq q{})) {
    $grep_result_200_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_200_1 > 0 ? 0 : 1;
    $output_200 = $grep_result_200_1;
    $output_200 = $grep_result_200_1;
    if ((scalar @grep_filtered_200_1) == 0) {
        $pipeline_success_200 = 0;
    }
    if ($output_200 ne q{} && !defined $output_printed_200) {
        print $output_200;
        if (!($output_200 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_200 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
do {
    my $output_201 = q{};
    my $output_printed_201;
    my $pipeline_success_201 = 1;
    $output_201 .= 'text with pattern in it' . "\n";
if ( !($output_201 =~ m{\n\z}) ) { $output_201 .= "\n"; }

        my $grep_result_201_1;
    my @grep_lines_201_1 = split /\n/msx, $output_201;
    my @grep_filtered_201_1 = grep { /pattern/msx } @grep_lines_201_1;
    my @grep_numbered_201_1;
    for my $i (0..@grep_lines_201_1-1) {
    if (scalar grep { $_ eq $grep_lines_201_1[$i] } @grep_filtered_201_1) {
    push @grep_numbered_201_1, sprintf "%d:%s", $i + 1, $grep_lines_201_1[$i];
    }
    }
    $grep_result_201_1 = join "\n", @grep_numbered_201_1;
    $CHILD_ERROR = scalar @grep_filtered_201_1 > 0 ? 0 : 1;
    $output_201 = $grep_result_201_1;
    $output_201 = $grep_result_201_1;
    if ((scalar @grep_filtered_201_1) == 0) {
        $pipeline_success_201 = 0;
    }
    if ($output_201 ne q{} && !defined $output_printed_201) {
        print $output_201;
        if (!($output_201 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_201 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
say "== Recursive and include/exclude parameters ==";
use File::Path qw(make_path);
my $err;
if ( !-d 'test_dir' ) {
    make_path( 'test_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_dir' . ": $err->[0]\n";
    }
}
open my $fh, '>', 'test_dir/file1.txt' or die "test_dir/file1.txt: $!\n";
say {*fh} "pattern here";
close $fh;
open my $fh, '>', 'test_dir/file2.txt' or die "test_dir/file2.txt: $!\n";
say {*fh} "no pattern";
close $fh;
my $grep_result_203;
my @grep_lines_203 = ();
my @grep_filenames_203 = ();
my $find_files_recursive_203;
$find_files_recursive_203 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_203->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_203 = $find_files_recursive_203->('test_dir', '*');
for my $file (@files_203) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_203, $line;
            push @grep_filenames_203, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_203 = grep { /pattern/msx } @grep_lines_203;
my @grep_with_filename_203;
for my $i (0..@grep_lines_203-1) {
    if (scalar grep { $_ eq $grep_lines_203[$i] } @grep_filtered_203) {
        push @grep_with_filename_203, $grep_filenames_203[$i] . ':' . $grep_lines_203[$i];
    }
}
$grep_result_203 = join "\n", @grep_with_filename_203;
if (!($grep_result_203 =~ m{\n\z} || $grep_result_203 eq q{})) {
    $grep_result_203 .= "\n";
}
print $grep_result_203;
$CHILD_ERROR = scalar @grep_filtered_203 > 0 ? 0 : 1;
my $grep_result_204;
my @grep_lines_204 = ();
my @grep_filenames_204 = ();
my $find_files_recursive_204;
$find_files_recursive_204 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_204->($path, $pattern));
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
my @files_204 = $find_files_recursive_204->('test_dir', '*.txt');
for my $file (@files_204) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_204, $line;
            push @grep_filenames_204, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_204 = grep { /pattern/msx } @grep_lines_204;
my @grep_with_filename_204;
for my $i (0..@grep_lines_204-1) {
    if (scalar grep { $_ eq $grep_lines_204[$i] } @grep_filtered_204) {
        push @grep_with_filename_204, $grep_filenames_204[$i] . ':' . $grep_lines_204[$i];
    }
}
$grep_result_204 = join "\n", @grep_with_filename_204;
if (!($grep_result_204 =~ m{\n\z} || $grep_result_204 eq q{})) {
    $grep_result_204 .= "\n";
}
print $grep_result_204;
$CHILD_ERROR = scalar @grep_filtered_204 > 0 ? 0 : 1;
my $grep_result_205;
my @grep_lines_205 = ();
my @grep_filenames_205 = ();
my $find_files_recursive_205;
$find_files_recursive_205 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_205->($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx && $file !~ /.*[.]bak$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_205 = $find_files_recursive_205->('test_dir', '*');
for my $file (@files_205) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_205, $line;
            push @grep_filenames_205, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_205 = grep { /pattern/msx } @grep_lines_205;
my @grep_with_filename_205;
for my $i (0..@grep_lines_205-1) {
    if (scalar grep { $_ eq $grep_lines_205[$i] } @grep_filtered_205) {
        push @grep_with_filename_205, $grep_filenames_205[$i] . ':' . $grep_lines_205[$i];
    }
}
$grep_result_205 = join "\n", @grep_with_filename_205;
if (!($grep_result_205 =~ m{\n\z} || $grep_result_205 eq q{})) {
    $grep_result_205 .= "\n";
}
print $grep_result_205;
$CHILD_ERROR = scalar @grep_filtered_205 > 0 ? 0 : 1;
my $grep_result_206;
my @grep_lines_206 = ();
my @grep_filenames_206 = ();
my $find_files_recursive_206;
$find_files_recursive_206 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_206->($path, $pattern));
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
my @files_206 = $find_files_recursive_206->('test_dir', '*.txt');
for my $file (@files_206) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_206, $line;
            push @grep_filenames_206, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_206 = grep { /pattern/msx } @grep_lines_206;
my %file_counts_206;
my @file_order_206;
for my $i (0..@grep_lines_206-1) {
    if (scalar grep { $_ eq $grep_lines_206[$i] } @grep_filtered_206) {
        my $f_206 = $grep_filenames_206[$i];
        push @file_order_206, $f_206 unless exists $file_counts_206{$f_206};
        $file_counts_206{$f_206}++;
    }
}
$grep_result_206 = q{};
for my $file (@file_order_206) {
    $grep_result_206 .= "$file:$file_counts_206{$file}\n";
}
print $grep_result_206;
$CHILD_ERROR = scalar @grep_filtered_206 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
do {
    my $output_207 = q{};
    my $output_printed_207;
    my $pipeline_success_207 = 1;
        my $grep_result_207_0;
    my @grep_lines_207_0 = ();
    my @grep_filenames_207_0 = ();
    my $find_files_recursive_207_0;
    $find_files_recursive_207_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_207_0->($path, $pattern));
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
    my @files_207_0 = $find_files_recursive_207_0->('test_dir', '*.txt');
    for my $file (@files_207_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_207_0, $line;
    push @grep_filenames_207_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_207_0 = grep { /pattern/msx } @grep_lines_207_0;
    my @grep_with_filename_207_0;
    for my $i (0..@grep_lines_207_0-1) {
    if (scalar grep { $_ eq $grep_lines_207_0[$i] } @grep_filtered_207_0) {
    push @grep_with_filename_207_0, $grep_filenames_207_0[$i] . ':' . $grep_lines_207_0[$i];
    }
    }
    $grep_result_207_0 = join "\n", @grep_with_filename_207_0;
    if (!($grep_result_207_0 =~ m{\n\z} || $grep_result_207_0 eq q{})) {
    $grep_result_207_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_207_0 > 0 ? 0 : 1;
    $output_207 = $grep_result_207_0;
    $output_207 = $grep_result_207_0;

        my $output_207_1 = do {
    my $_wc_data = $output_207;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = sprintf("%d \n", $_wc_lines);
    $_wc_result;
    };
    $output_207 = $output_207_1;
    if ($output_207 ne q{} && !defined $output_printed_207) {
        print $output_207;
        if (!($output_207 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_207 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
say "== Advanced parameters ==";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
do {
    my $output_208 = q{};
    my $output_printed_208;
    my $pipeline_success_208 = 1;
    $output_208 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_208 =~ m{\n\z}) ) { $output_208 .= "\n"; }

        my $grep_result_208_1;
    my @grep_lines_208_1 = split /\n/msx, $output_208;
    my @grep_filtered_208_1 = grep { /match/msx } @grep_lines_208_1;
    @grep_filtered_208_1 = @grep_filtered_208_1[0..1];
    $grep_result_208_1 = join "\n", @grep_filtered_208_1;
    if (!($grep_result_208_1 =~ m{\n\z} || $grep_result_208_1 eq q{})) {
    $grep_result_208_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_208_1 > 0 ? 0 : 1;
    $output_208 = $grep_result_208_1;
    $output_208 = $grep_result_208_1;
    if ((scalar @grep_filtered_208_1) == 0) {
        $pipeline_success_208 = 0;
    }
    if ($output_208 ne q{} && !defined $output_printed_208) {
        print $output_208;
        if (!($output_208 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_208 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if (do {
do {
    my $output_209 = q{};
    my $output_printed_209;
    my $pipeline_success_209 = 1;
    $output_209 .= 'text with pattern in it' . "\n";
if ( !($output_209 =~ m{\n\z}) ) { $output_209 .= "\n"; }

        my $grep_result_209_1;
    my @grep_lines_209_1 = split /\n/msx, $output_209;
    my @grep_filtered_209_1 = grep { /pattern/msx } @grep_lines_209_1;
    $grep_result_209_1 = join "\n", @grep_filtered_209_1;
    if (!($grep_result_209_1 =~ m{\n\z} || $grep_result_209_1 eq q{})) {
    $grep_result_209_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_209_1 > 0 ? 0 : 1;
    $grep_result_209_1 = q{};
    $output_209 = q{};
    if ((scalar @grep_filtered_209_1) == 0) {
        $pipeline_success_209 = 0;
    }
    if ($output_209 ne q{} && !defined $output_printed_209) {
        print $output_209;
        if (!($output_209 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_209 ) { $main_exit_code = 1; }
    }
    $CHILD_ERROR == 0
}) {
        say "found";
}
if ($CHILD_ERROR != 0) {
        say "not found";
}
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
do {
    my $output_210 = q{};
    my $output_printed_210;
    my $pipeline_success_210 = 1;
        my $grep_result_210_0;
    my @grep_lines_210_0 = ();
    my @grep_filenames_210_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_210_0, $line;
    push @grep_filenames_210_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_210_0 = grep { /pattern/msx } @grep_lines_210_0;
    $grep_result_210_0 = @grep_filtered_210_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_210_0 > 0 ? 0 : 1;
    $output_210 = $grep_result_210_0;
    $output_210 = $grep_result_210_0;

        my $set1_211 = "\\0";
    my $set2_211 = "\\n";
    my $input_211 = $output_210;
    # Expand character ranges for tr command
    my $expanded_set1_211 = $set1_211;
    my $expanded_set2_211 = $set2_211;
    # Handle a-z range in set1
    if ($expanded_set1_211 =~ /a-z/msx) {
    $expanded_set1_211 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_211 =~ /A-Z/msx) {
    $expanded_set1_211 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_211 =~ /\[:upper:\]/msx) {
    $expanded_set1_211 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_211 =~ /\[:lower:\]/msx) {
    $expanded_set1_211 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_211 =~ /a-z/msx) {
    $expanded_set2_211 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_211 =~ /A-Z/msx) {
    $expanded_set2_211 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_211 =~ /\[:upper:\]/msx) {
    $expanded_set2_211 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_211 =~ /\[:lower:\]/msx) {
    $expanded_set2_211 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_210_1 = q{};
    for my $char ( split //msx, $input_211 ) {
    my $pos_211 = index $expanded_set1_211, $char;
    if ( $pos_211 >= 0 && $pos_211 < length $expanded_set2_211 ) {
    $tr_result_210_1 .= substr $expanded_set2_211, $pos_211, 1;
    } else {
    $tr_result_210_1 .= $char;
    }
    }
    if (!($tr_result_210_1 =~ m{\n\z} || $tr_result_210_1 eq q{})) {
    $tr_result_210_1 .= "\n";
    }
    $output_210 = $tr_result_210_1;
    $output_210 = $tr_result_210_1;
    if ($output_210 ne q{} && !defined $output_printed_210) {
        print $output_210;
        if (!($output_210 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_210 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
unlink('temp_file.txt');
if ( -e "test_dir" ) {
    if ( -d "test_dir" ) {
        my $err;
        require File::Path;
        File::Path::remove_tree("test_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_dir", ": $err->[0]\n";
        }
        else {
                    }
    }
    else {
        if ( unlink "test_dir" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "test_dir",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

exit $main_exit_code;

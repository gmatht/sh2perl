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

$PROGRAM_NAME = '018_grep_params.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Basic grep parameters ==\n";
# Original bash: echo "text with pattern" | grep -i "PATTERN"
{
    my $output_189 = q{};
    my $output_printed_189;
    my $pipeline_success_189 = 1;
    $output_189 .= 'text with pattern' . "\n";
if ( !($output_189 =~ m{\n\z}msx) ) { $output_189 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_189_1;
    my @grep_lines_189_1 = split /\n/msx, $output_189;
    my @grep_filtered_189_1 = grep { /PATTERN/msxi } @grep_lines_189_1;
    $grep_result_189_1 = join "\n", @grep_filtered_189_1;
    if (!($grep_result_189_1 =~ m{\n\z}msx || $grep_result_189_1 eq q{})) {
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
        if (!($output_189 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_189 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_190 = q{};
    my $output_printed_190;
    my $pipeline_success_190 = 1;
    $output_190 .= "line1\nline2\nline3";
if ( !($output_190 =~ m{\n\z}msx) ) { $output_190 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_190_1;
    my @grep_lines_190_1 = split /\n/msx, $output_190;
    my @grep_filtered_190_1 = grep { !/line2/msx } @grep_lines_190_1;
    $grep_result_190_1 = join "\n", @grep_filtered_190_1;
    if (!($grep_result_190_1 =~ m{\n\z}msx || $grep_result_190_1 eq q{})) {
    $grep_result_190_1 .= "\n";
    }
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
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_191 = q{};
    my $output_printed_191;
    my $pipeline_success_191 = 1;
    $output_191 .= "match\nno match\nmatch again";
if ( !($output_191 =~ m{\n\z}msx) ) { $output_191 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_191_1;
    my @grep_lines_191_1 = split /\n/msx, $output_191;
    my @grep_filtered_191_1 = grep { /match/msx } @grep_lines_191_1;
    $grep_result_191_1 = scalar @grep_filtered_191_1 . "\n";
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
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_192 = q{};
    my $output_printed_192;
    my $pipeline_success_192 = 1;
    $output_192 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_192 =~ m{\n\z}msx) ) { $output_192 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_192_1;
    my @grep_lines_192_1 = split /\n/msx, $output_192;
    my @grep_filtered_192_1 = grep { /TARGET/msx } @grep_lines_192_1;
    my @grep_with_context_192_1;
    for my $i (0..@grep_lines_192_1-1) {
    if (scalar grep { $_ eq $grep_lines_192_1[$i] } @grep_filtered_192_1) {
    push @grep_with_context_192_1, $grep_lines_192_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_192_1, $grep_lines_192_1[$j];
    }
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
        if (!($output_192 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_192 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
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
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_193_1, $grep_lines_193_1[$j];
    }
    }
    push @grep_with_context_193_1, $grep_lines_193_1[$i];
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
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
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
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_194_1, $grep_lines_194_1[$j];
    }
    }
    push @grep_with_context_194_1, $grep_lines_194_1[$i];
    for my $j (($i + 1)..($i + 1)) {
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
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== File handling parameters ==\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'temp_file.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $grep_result_195;
my @grep_lines_195 = ();
my @grep_filenames_195 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
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
my @grep_with_filename_195;
for my $line (@grep_filtered_195) {
    push @grep_with_filename_195, "temp_file.txt:$line";
}
$grep_result_195 = join "\n", @grep_with_filename_195;
if (!($grep_result_195 =~ m{\n\z}msx || $grep_result_195 eq q{})) {
    $grep_result_195 .= "\n";
}
print $grep_result_195;
$CHILD_ERROR = scalar @grep_filtered_195 > 0 ? 0 : 1;
my $grep_result_196;
my @grep_lines_196 = ();
my @grep_filenames_196 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
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
$grep_result_196 = join "\n", @grep_filtered_196;
if (!($grep_result_196 =~ m{\n\z}msx || $grep_result_196 eq q{})) {
    $grep_result_196 .= "\n";
}
print $grep_result_196;
$CHILD_ERROR = scalar @grep_filtered_196 > 0 ? 0 : 1;
my $grep_result_197;
my @grep_lines_197 = ();
my @grep_filenames_197 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_197, $line;
        push @grep_filenames_197, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_197 = grep { /content/msx } @grep_lines_197;
$grep_result_197 = @grep_filtered_197 > 0 ? "temp_file.txt" : "";
print $grep_result_197;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_197 > 0 ? 0 : 1;
my $grep_result_198;
my @grep_lines_198 = ();
my @grep_filenames_198 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_198, $line;
        push @grep_filenames_198, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_198 = grep { /nonexistent/msx } @grep_lines_198;
$grep_result_198 = @grep_filtered_198 == 0 ? "temp_file.txt" : "";
print $grep_result_198;
print "\n";
$CHILD_ERROR = $grep_result_198 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
{
    my $output_200 = q{};
    my $output_printed_200;
    my $pipeline_success_200 = 1;
    $output_200 .= 'text with pattern in it' . "\n";
if ( !($output_200 =~ m{\n\z}msx) ) { $output_200 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_200_1;
    my @grep_lines_200_1 = split /\n/msx, $output_200;
    my @grep_filtered_200_1 = grep { /pattern/msx } @grep_lines_200_1;
    my @grep_matches_200_1;
    foreach my $line (@grep_filtered_200_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_200_1, $1;
    }
    }
    $grep_result_200_1 = join "\n", @grep_matches_200_1;
    $CHILD_ERROR = scalar @grep_filtered_200_1 > 0 ? 0 : 1;
    $output_200 = $grep_result_200_1;
    $output_200 = $grep_result_200_1;
    if ((scalar @grep_filtered_200_1) == 0) {
        $pipeline_success_200 = 0;
    }
    if ($output_200 ne q{} && !defined $output_printed_200) {
        print $output_200;
        if (!($output_200 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_200 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_201 = q{};
    my $output_printed_201;
    my $pipeline_success_201 = 1;
    $output_201 .= 'text with pattern in it' . "\n";
if ( !($output_201 =~ m{\n\z}msx) ) { $output_201 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_201_1;
    my @grep_lines_201_1 = split /\n/msx, $output_201;
    my @grep_filtered_201_1 = grep { /pattern/msx } @grep_lines_201_1;
    my @grep_with_offset_201_1;
    my $offset_201_1 = 0;
    for my $line (@grep_lines_201_1) {
    if (grep { $_ eq $line } @grep_filtered_201_1) {
    push @grep_with_offset_201_1, sprintf "%d:%s", $offset_201_1, $line;
    }
    $offset_201_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_201_1 = join "\n", @grep_with_offset_201_1;
    if (!($grep_result_201_1 =~ m{\n\z}msx || $grep_result_201_1 eq q{})) {
    $grep_result_201_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_201_1 > 0 ? 0 : 1;
    $output_201 = $grep_result_201_1;
    $output_201 = $grep_result_201_1;
    if ((scalar @grep_filtered_201_1) == 0) {
        $pipeline_success_201 = 0;
    }
    if ($output_201 ne q{} && !defined $output_printed_201) {
        print $output_201;
        if (!($output_201 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_201 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_202 = q{};
    my $output_printed_202;
    my $pipeline_success_202 = 1;
    $output_202 .= 'text with pattern in it' . "\n";
if ( !($output_202 =~ m{\n\z}msx) ) { $output_202 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_202_1;
    my @grep_lines_202_1 = split /\n/msx, $output_202;
    my @grep_filtered_202_1 = grep { /pattern/msx } @grep_lines_202_1;
    my @grep_numbered_202_1;
    for my $i (0..@grep_lines_202_1-1) {
    if (scalar grep { $_ eq $grep_lines_202_1[$i] } @grep_filtered_202_1) {
    push @grep_numbered_202_1, sprintf "%d:%s", $i + 1, $grep_lines_202_1[$i];
    }
    }
    $grep_result_202_1 = join "\n", @grep_numbered_202_1;
    $CHILD_ERROR = scalar @grep_filtered_202_1 > 0 ? 0 : 1;
    $output_202 = $grep_result_202_1;
    $output_202 = $grep_result_202_1;
    if ((scalar @grep_filtered_202_1) == 0) {
        $pipeline_success_202 = 0;
    }
    if ($output_202 ne q{} && !defined $output_printed_202) {
        print $output_202;
        if (!($output_202 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_202 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Recursive and include/exclude parameters ==\n";
use File::Path qw(make_path);
my $err;
if ( !-d 'test_dir' ) {
    make_path( 'test_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_dir' . ": $err->[0]\n";
    }
}
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_dir/file1.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "pattern here\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_dir/file2.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "no pattern\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
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
                if ($file =~ /[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_204 = $find_files_recursive_204->('test_dir', '*');
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
if (!($grep_result_204 =~ m{\n\z}msx || $grep_result_204 eq q{})) {
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
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_205 = $find_files_recursive_205->('test_dir', '*.txt');
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
if (!($grep_result_205 =~ m{\n\z}msx || $grep_result_205 eq q{})) {
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
                if ($file =~ /[.]txt$/msx && $file !~ /.*[.]bak$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_206 = $find_files_recursive_206->('test_dir', '*');
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
my @grep_with_filename_206;
for my $i (0..@grep_lines_206-1) {
    if (scalar grep { $_ eq $grep_lines_206[$i] } @grep_filtered_206) {
        push @grep_with_filename_206, $grep_filenames_206[$i] . ':' . $grep_lines_206[$i];
    }
}
$grep_result_206 = join "\n", @grep_with_filename_206;
if (!($grep_result_206 =~ m{\n\z}msx || $grep_result_206 eq q{})) {
    $grep_result_206 .= "\n";
}
print $grep_result_206;
$CHILD_ERROR = scalar @grep_filtered_206 > 0 ? 0 : 1;
my $grep_result_207;
my @grep_lines_207 = ();
my @grep_filenames_207 = ();
my $find_files_recursive_207;
$find_files_recursive_207 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_207->($path, $pattern));
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
my @files_207 = $find_files_recursive_207->('test_dir', '*.txt');
for my $file (@files_207) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_207, $line;
            push @grep_filenames_207, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_207 = grep { /pattern/msx } @grep_lines_207;
my %file_counts_207;
my @file_order_207;
for my $i (0..@grep_lines_207-1) {
    if (scalar grep { $_ eq $grep_lines_207[$i] } @grep_filtered_207) {
        my $f_207 = $grep_filenames_207[$i];
        push @file_order_207, $f_207 unless exists $file_counts_207{$f_207};
        $file_counts_207{$f_207}++;
    }
}
$grep_result_207 = q{};
for my $file (@file_order_207) {
    $grep_result_207 .= "$file:$file_counts_207{$file}\n";
}
print $grep_result_207;
$CHILD_ERROR = scalar @grep_filtered_207 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_208 = q{};
    my $output_printed_208;
    my $pipeline_success_208 = 1;
        my $grep_result_208_0;
    my @grep_lines_208_0 = ();
    my @grep_filenames_208_0 = ();
    my $find_files_recursive_208_0;
    $find_files_recursive_208_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_208_0->($path, $pattern));
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
    my @files_208_0 = $find_files_recursive_208_0->('test_dir', '*.txt');
    for my $file (@files_208_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_208_0, $line;
    push @grep_filenames_208_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_208_0 = grep { /pattern/msx } @grep_lines_208_0;
    my @grep_with_filename_208_0;
    for my $i (0..@grep_lines_208_0-1) {
    if (scalar grep { $_ eq $grep_lines_208_0[$i] } @grep_filtered_208_0) {
    push @grep_with_filename_208_0, $grep_filenames_208_0[$i] . ':' . $grep_lines_208_0[$i];
    }
    }
    $grep_result_208_0 = join "\n", @grep_with_filename_208_0;
    if (!($grep_result_208_0 =~ m{\n\z}msx || $grep_result_208_0 eq q{})) {
    $grep_result_208_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_208_0 > 0 ? 0 : 1;
    $output_208 = $grep_result_208_0;
    $output_208 = $grep_result_208_0;

        use IPC::Open3;
    my @wc_args_208_1 = ('-l');
    my ($wc_in_208_1, $wc_out_208_1, $wc_err_208_1);
    my $wc_pid_208_1 = open3($wc_in_208_1, $wc_out_208_1, $wc_err_208_1, 'wc', @wc_args_208_1);
    print {$wc_in_208_1} $output_208;
    close $wc_in_208_1 or die "Close failed: $OS_ERROR\n";
    my $output_208_1 = do { local $/ = undef; <$wc_out_208_1> };
    if ($output_208_1 eq q{}) { $output_208_1 = "0\n"; }
    close $wc_out_208_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_208_1, 0;
    $output_208 = $output_208_1;
    if ($output_208 ne q{} && !defined $output_printed_208) {
        print $output_208;
        if (!($output_208 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_208 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_209 = q{};
    my $output_printed_209;
    my $pipeline_success_209 = 1;
    $output_209 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_209 =~ m{\n\z}msx) ) { $output_209 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_209_1;
    my @grep_lines_209_1 = split /\n/msx, $output_209;
    my @grep_filtered_209_1 = grep { /match/msx } @grep_lines_209_1;
    @grep_filtered_209_1 = @grep_filtered_209_1[0..1];
    $grep_result_209_1 = join "\n", @grep_filtered_209_1;
    if (!($grep_result_209_1 =~ m{\n\z}msx || $grep_result_209_1 eq q{})) {
    $grep_result_209_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_209_1 > 0 ? 0 : 1;
    $output_209 = $grep_result_209_1;
    $output_209 = $grep_result_209_1;
    if ((scalar @grep_filtered_209_1) == 0) {
        $pipeline_success_209 = 0;
    }
    if ($output_209 ne q{} && !defined $output_printed_209) {
        print $output_209;
        if (!($output_209 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_209 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if (do {
{
    my $output_210 = q{};
    my $output_printed_210;
    my $pipeline_success_210 = 1;
    $output_210 .= 'text with pattern in it' . "\n";
if ( !($output_210 =~ m{\n\z}msx) ) { $output_210 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_210_1;
    my @grep_lines_210_1 = split /\n/msx, $output_210;
    my @grep_filtered_210_1 = grep { /pattern/msx } @grep_lines_210_1;
    $grep_result_210_1 = join "\n", @grep_filtered_210_1;
    if (!($grep_result_210_1 =~ m{\n\z}msx || $grep_result_210_1 eq q{})) {
    $grep_result_210_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_210_1 > 0 ? 0 : 1;
    $grep_result_210_1 = q{};
    $output_210 = q{};
    if ((scalar @grep_filtered_210_1) == 0) {
        $pipeline_success_210 = 0;
    }
    if ($output_210 ne q{} && !defined $output_printed_210) {
        print $output_210;
        if (!($output_210 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_210 ) { $main_exit_code = 1; }
    }
    $CHILD_ERROR == 0
}) {
        print "found\n";
}
if ($CHILD_ERROR != 0) {
        print "not found\n";
}
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_211 = q{};
    my $output_printed_211;
    my $pipeline_success_211 = 1;
        my $grep_result_211_0;
    my @grep_lines_211_0 = ();
    my @grep_filenames_211_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_211_0, $line;
    push @grep_filenames_211_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_211_0 = grep { /pattern/msx } @grep_lines_211_0;
    $grep_result_211_0 = @grep_filtered_211_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_211_0 > 0 ? 0 : 1;
    $output_211 = $grep_result_211_0;
    $output_211 = $grep_result_211_0;

        my $set1_212 = "\\0";
    my $set2_212 = "\\n";
    my $input_212 = $output_211;
    # Expand character ranges for tr command
    my $expanded_set1_212 = $set1_212;
    my $expanded_set2_212 = $set2_212;
    # Handle a-z range in set1
    if ($expanded_set1_212 =~ /a-z/msx) {
    $expanded_set1_212 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_212 =~ /A-Z/msx) {
    $expanded_set1_212 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_212 =~ /a-z/msx) {
    $expanded_set2_212 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_212 =~ /A-Z/msx) {
    $expanded_set2_212 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_211_1 = q{};
    for my $char ( split //msx, $input_212 ) {
    my $pos_212 = index $expanded_set1_212, $char;
    if ( $pos_212 >= 0 && $pos_212 < length $expanded_set2_212 ) {
    $tr_result_211_1 .= substr $expanded_set2_212, $pos_212, 1;
    } else {
    $tr_result_211_1 .= $char;
    }
    }
    if (!($tr_result_211_1 =~ m{\n\z}msx || $tr_result_211_1 eq q{})) {
    $tr_result_211_1 .= "\n";
    }
    $output_211 = $tr_result_211_1;
    $output_211 = $tr_result_211_1;
    if ($output_211 ne q{} && !defined $output_printed_211) {
        print $output_211;
        if (!($output_211 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_211 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if ( -e "temp_file.txt" ) {
    if ( -d "temp_file.txt" ) {
        carp "rm: carping: ", "temp_file.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp_file.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "temp_file.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
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

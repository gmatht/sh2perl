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

$PROGRAM_NAME = '018_grep_params.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Basic grep parameters ==\n";
# Original bash: echo "text with pattern" | grep -i "PATTERN"
{
    my $output_199 = q{};
    my $output_printed_199;
    my $pipeline_success_199 = 1;
    $output_199 .= 'text with pattern' . "\n";
if ( !($output_199 =~ m{\n\z}msx) ) { $output_199 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_199_1;
    my @grep_lines_199_1 = split /\n/msx, $output_199;
    my @grep_filtered_199_1 = grep { /PATTERN/msxi } @grep_lines_199_1;
    $grep_result_199_1 = join "\n", @grep_filtered_199_1;
    if (!($grep_result_199_1 =~ m{\n\z}msx || $grep_result_199_1 eq q{})) {
    $grep_result_199_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_199_1 > 0 ? 0 : 1;
    $output_199 = $grep_result_199_1;
    $output_199 = $grep_result_199_1;
    if ((scalar @grep_filtered_199_1) == 0) {
        $pipeline_success_199 = 0;
    }
    if ($output_199 ne q{} && !defined $output_printed_199) {
        print $output_199;
        if (!($output_199 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_199 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_200 = q{};
    my $output_printed_200;
    my $pipeline_success_200 = 1;
    $output_200 .= "line1\nline2\nline3";
if ( !($output_200 =~ m{\n\z}msx) ) { $output_200 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_200_1;
    my @grep_lines_200_1 = split /\n/msx, $output_200;
    my @grep_filtered_200_1 = grep { !/line2/msx } @grep_lines_200_1;
    $grep_result_200_1 = join "\n", @grep_filtered_200_1;
    if (!($grep_result_200_1 =~ m{\n\z}msx || $grep_result_200_1 eq q{})) {
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
        if (!($output_200 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_200 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_201 = q{};
    my $output_printed_201;
    my $pipeline_success_201 = 1;
    $output_201 .= "match\nno match\nmatch again";
if ( !($output_201 =~ m{\n\z}msx) ) { $output_201 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_201_1;
    my @grep_lines_201_1 = split /\n/msx, $output_201;
    my @grep_filtered_201_1 = grep { /match/msx } @grep_lines_201_1;
    $grep_result_201_1 = scalar @grep_filtered_201_1 . "\n";
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
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_202 = q{};
    my $output_printed_202;
    my $pipeline_success_202 = 1;
    $output_202 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_202 =~ m{\n\z}msx) ) { $output_202 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_202_1;
    my @grep_lines_202_1 = split /\n/msx, $output_202;
    my @grep_filtered_202_1 = grep { /TARGET/msx } @grep_lines_202_1;
    my @grep_with_context_202_1;
    for my $i (0..@grep_lines_202_1-1) {
    if (scalar grep { $_ eq $grep_lines_202_1[$i] } @grep_filtered_202_1) {
    push @grep_with_context_202_1, $grep_lines_202_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_202_1, $grep_lines_202_1[$j];
    }
    }
    }
    $grep_result_202_1 = join "\n", @grep_with_context_202_1;
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_203 = q{};
    my $output_printed_203;
    my $pipeline_success_203 = 1;
    $output_203 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_203 =~ m{\n\z}msx) ) { $output_203 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_203_1;
    my @grep_lines_203_1 = split /\n/msx, $output_203;
    my @grep_filtered_203_1 = grep { /TARGET/msx } @grep_lines_203_1;
    my @grep_with_context_203_1;
    for my $i (0..@grep_lines_203_1-1) {
    if (scalar grep { $_ eq $grep_lines_203_1[$i] } @grep_filtered_203_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_203_1, $grep_lines_203_1[$j];
    }
    }
    push @grep_with_context_203_1, $grep_lines_203_1[$i];
    }
    }
    $grep_result_203_1 = join "\n", @grep_with_context_203_1;
    $CHILD_ERROR = scalar @grep_filtered_203_1 > 0 ? 0 : 1;
    $output_203 = $grep_result_203_1;
    $output_203 = $grep_result_203_1;
    if ((scalar @grep_filtered_203_1) == 0) {
        $pipeline_success_203 = 0;
    }
    if ($output_203 ne q{} && !defined $output_printed_203) {
        print $output_203;
        if (!($output_203 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_203 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_204 = q{};
    my $output_printed_204;
    my $pipeline_success_204 = 1;
    $output_204 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_204 =~ m{\n\z}msx) ) { $output_204 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_204_1;
    my @grep_lines_204_1 = split /\n/msx, $output_204;
    my @grep_filtered_204_1 = grep { /TARGET/msx } @grep_lines_204_1;
    my @grep_with_context_204_1;
    for my $i (0..@grep_lines_204_1-1) {
    if (scalar grep { $_ eq $grep_lines_204_1[$i] } @grep_filtered_204_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_204_1, $grep_lines_204_1[$j];
    }
    }
    push @grep_with_context_204_1, $grep_lines_204_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_204_1, $grep_lines_204_1[$j];
    }
    }
    }
    $grep_result_204_1 = join "\n", @grep_with_context_204_1;
    $CHILD_ERROR = scalar @grep_filtered_204_1 > 0 ? 0 : 1;
    $output_204 = $grep_result_204_1;
    $output_204 = $grep_result_204_1;
    if ((scalar @grep_filtered_204_1) == 0) {
        $pipeline_success_204 = 0;
    }
    if ($output_204 ne q{} && !defined $output_printed_204) {
        print $output_204;
        if (!($output_204 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_204 ) { $main_exit_code = 1; }
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
my $grep_result_205;
my @grep_lines_205 = ();
my @grep_filenames_205 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_205, $line;
        push @grep_filenames_205, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_205 = grep { /content/msx } @grep_lines_205;
my @grep_with_filename_205;
for my $line (@grep_filtered_205) {
    push @grep_with_filename_205, "temp_file.txt:$line";
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
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_206, $line;
        push @grep_filenames_206, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_206 = grep { /content/msx } @grep_lines_206;
$grep_result_206 = join "\n", @grep_filtered_206;
if (!($grep_result_206 =~ m{\n\z}msx || $grep_result_206 eq q{})) {
    $grep_result_206 .= "\n";
}
print $grep_result_206;
$CHILD_ERROR = scalar @grep_filtered_206 > 0 ? 0 : 1;
my $grep_result_207;
my @grep_lines_207 = ();
my @grep_filenames_207 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_207, $line;
        push @grep_filenames_207, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_207 = grep { /content/msx } @grep_lines_207;
$grep_result_207 = @grep_filtered_207 > 0 ? "temp_file.txt" : "";
print $grep_result_207;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_207 > 0 ? 0 : 1;
my $grep_result_208;
my @grep_lines_208 = ();
my @grep_filenames_208 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_208, $line;
        push @grep_filenames_208, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_208 = grep { /nonexistent/msx } @grep_lines_208;
$grep_result_208 = @grep_filtered_208 == 0 ? "temp_file.txt" : "";
print $grep_result_208;
print "\n";
$CHILD_ERROR = $grep_result_208 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
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
    my @grep_matches_210_1;
    foreach my $line (@grep_filtered_210_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_210_1, $1;
    }
    }
    $grep_result_210_1 = join "\n", @grep_matches_210_1;
    $CHILD_ERROR = scalar @grep_filtered_210_1 > 0 ? 0 : 1;
    $output_210 = $grep_result_210_1;
    $output_210 = $grep_result_210_1;
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
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_211 = q{};
    my $output_printed_211;
    my $pipeline_success_211 = 1;
    $output_211 .= 'text with pattern in it' . "\n";
if ( !($output_211 =~ m{\n\z}msx) ) { $output_211 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_211_1;
    my @grep_lines_211_1 = split /\n/msx, $output_211;
    my @grep_filtered_211_1 = grep { /pattern/msx } @grep_lines_211_1;
    my @grep_with_offset_211_1;
    my $offset_211_1 = 0;
    for my $line (@grep_lines_211_1) {
    if (grep { $_ eq $line } @grep_filtered_211_1) {
    push @grep_with_offset_211_1, sprintf "%d:%s", $offset_211_1, $line;
    }
    $offset_211_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_211_1 = join "\n", @grep_with_offset_211_1;
    if (!($grep_result_211_1 =~ m{\n\z}msx || $grep_result_211_1 eq q{})) {
    $grep_result_211_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_211_1 > 0 ? 0 : 1;
    $output_211 = $grep_result_211_1;
    $output_211 = $grep_result_211_1;
    if ((scalar @grep_filtered_211_1) == 0) {
        $pipeline_success_211 = 0;
    }
    if ($output_211 ne q{} && !defined $output_printed_211) {
        print $output_211;
        if (!($output_211 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_211 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_212 = q{};
    my $output_printed_212;
    my $pipeline_success_212 = 1;
    $output_212 .= 'text with pattern in it' . "\n";
if ( !($output_212 =~ m{\n\z}msx) ) { $output_212 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_212_1;
    my @grep_lines_212_1 = split /\n/msx, $output_212;
    my @grep_filtered_212_1 = grep { /pattern/msx } @grep_lines_212_1;
    my @grep_numbered_212_1;
    for my $i (0..@grep_lines_212_1-1) {
    if (scalar grep { $_ eq $grep_lines_212_1[$i] } @grep_filtered_212_1) {
    push @grep_numbered_212_1, sprintf "%d:%s", $i + 1, $grep_lines_212_1[$i];
    }
    }
    $grep_result_212_1 = join "\n", @grep_numbered_212_1;
    $CHILD_ERROR = scalar @grep_filtered_212_1 > 0 ? 0 : 1;
    $output_212 = $grep_result_212_1;
    $output_212 = $grep_result_212_1;
    if ((scalar @grep_filtered_212_1) == 0) {
        $pipeline_success_212 = 0;
    }
    if ($output_212 ne q{} && !defined $output_printed_212) {
        print $output_212;
        if (!($output_212 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_212 ) { $main_exit_code = 1; }
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
my $grep_result_214;
my @grep_lines_214 = ();
my @grep_filenames_214 = ();
my $find_files_recursive_214;
$find_files_recursive_214 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_214->($path, $pattern));
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
my @files_214 = $find_files_recursive_214->('test_dir', '*');
for my $file (@files_214) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_214, $line;
            push @grep_filenames_214, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_214 = grep { /pattern/msx } @grep_lines_214;
my @grep_with_filename_214;
for my $i (0..@grep_lines_214-1) {
    if (scalar grep { $_ eq $grep_lines_214[$i] } @grep_filtered_214) {
        push @grep_with_filename_214, $grep_filenames_214[$i] . ':' . $grep_lines_214[$i];
    }
}
$grep_result_214 = join "\n", @grep_with_filename_214;
if (!($grep_result_214 =~ m{\n\z}msx || $grep_result_214 eq q{})) {
    $grep_result_214 .= "\n";
}
print $grep_result_214;
$CHILD_ERROR = scalar @grep_filtered_214 > 0 ? 0 : 1;
my $grep_result_215;
my @grep_lines_215 = ();
my @grep_filenames_215 = ();
my $find_files_recursive_215;
$find_files_recursive_215 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_215->($path, $pattern));
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
my @files_215 = $find_files_recursive_215->('test_dir', '*.txt');
for my $file (@files_215) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_215, $line;
            push @grep_filenames_215, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_215 = grep { /pattern/msx } @grep_lines_215;
my @grep_with_filename_215;
for my $i (0..@grep_lines_215-1) {
    if (scalar grep { $_ eq $grep_lines_215[$i] } @grep_filtered_215) {
        push @grep_with_filename_215, $grep_filenames_215[$i] . ':' . $grep_lines_215[$i];
    }
}
$grep_result_215 = join "\n", @grep_with_filename_215;
if (!($grep_result_215 =~ m{\n\z}msx || $grep_result_215 eq q{})) {
    $grep_result_215 .= "\n";
}
print $grep_result_215;
$CHILD_ERROR = scalar @grep_filtered_215 > 0 ? 0 : 1;
my $grep_result_216;
my @grep_lines_216 = ();
my @grep_filenames_216 = ();
my $find_files_recursive_216;
$find_files_recursive_216 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_216->($path, $pattern));
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
my @files_216 = $find_files_recursive_216->('test_dir', '*');
for my $file (@files_216) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_216, $line;
            push @grep_filenames_216, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_216 = grep { /pattern/msx } @grep_lines_216;
my @grep_with_filename_216;
for my $i (0..@grep_lines_216-1) {
    if (scalar grep { $_ eq $grep_lines_216[$i] } @grep_filtered_216) {
        push @grep_with_filename_216, $grep_filenames_216[$i] . ':' . $grep_lines_216[$i];
    }
}
$grep_result_216 = join "\n", @grep_with_filename_216;
if (!($grep_result_216 =~ m{\n\z}msx || $grep_result_216 eq q{})) {
    $grep_result_216 .= "\n";
}
print $grep_result_216;
$CHILD_ERROR = scalar @grep_filtered_216 > 0 ? 0 : 1;
my $grep_result_217;
my @grep_lines_217 = ();
my @grep_filenames_217 = ();
my $find_files_recursive_217;
$find_files_recursive_217 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_217->($path, $pattern));
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
my @files_217 = $find_files_recursive_217->('test_dir', '*.txt');
for my $file (@files_217) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_217, $line;
            push @grep_filenames_217, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_217 = grep { /pattern/msx } @grep_lines_217;
my %file_counts_217;
my @file_order_217;
for my $i (0..@grep_lines_217-1) {
    if (scalar grep { $_ eq $grep_lines_217[$i] } @grep_filtered_217) {
        my $f_217 = $grep_filenames_217[$i];
        push @file_order_217, $f_217 unless exists $file_counts_217{$f_217};
        $file_counts_217{$f_217}++;
    }
}
$grep_result_217 = q{};
for my $file (@file_order_217) {
    $grep_result_217 .= "$file:$file_counts_217{$file}\n";
}
print $grep_result_217;
$CHILD_ERROR = scalar @grep_filtered_217 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_218 = q{};
    my $output_printed_218;
    my $pipeline_success_218 = 1;
        my $grep_result_218_0;
    my @grep_lines_218_0 = ();
    my @grep_filenames_218_0 = ();
    my $find_files_recursive_218_0;
    $find_files_recursive_218_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_218_0->($path, $pattern));
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
    my @files_218_0 = $find_files_recursive_218_0->('test_dir', '*.txt');
    for my $file (@files_218_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_218_0, $line;
    push @grep_filenames_218_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_218_0 = grep { /pattern/msx } @grep_lines_218_0;
    my @grep_with_filename_218_0;
    for my $i (0..@grep_lines_218_0-1) {
    if (scalar grep { $_ eq $grep_lines_218_0[$i] } @grep_filtered_218_0) {
    push @grep_with_filename_218_0, $grep_filenames_218_0[$i] . ':' . $grep_lines_218_0[$i];
    }
    }
    $grep_result_218_0 = join "\n", @grep_with_filename_218_0;
    if (!($grep_result_218_0 =~ m{\n\z}msx || $grep_result_218_0 eq q{})) {
    $grep_result_218_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_218_0 > 0 ? 0 : 1;
    $output_218 = $grep_result_218_0;
    $output_218 = $grep_result_218_0;

        my $output_218_1 = do {
    my $_wc_data = $output_218;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_lines;
    $_wc_result .= "\n";
    $_wc_result;
    };
    $output_218 = $output_218_1;
    if ($output_218 ne q{} && !defined $output_printed_218) {
        print $output_218;
        if (!($output_218 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_218 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_219 = q{};
    my $output_printed_219;
    my $pipeline_success_219 = 1;
    $output_219 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_219 =~ m{\n\z}msx) ) { $output_219 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_219_1;
    my @grep_lines_219_1 = split /\n/msx, $output_219;
    my @grep_filtered_219_1 = grep { /match/msx } @grep_lines_219_1;
    @grep_filtered_219_1 = @grep_filtered_219_1[0..1];
    $grep_result_219_1 = join "\n", @grep_filtered_219_1;
    if (!($grep_result_219_1 =~ m{\n\z}msx || $grep_result_219_1 eq q{})) {
    $grep_result_219_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_219_1 > 0 ? 0 : 1;
    $output_219 = $grep_result_219_1;
    $output_219 = $grep_result_219_1;
    if ((scalar @grep_filtered_219_1) == 0) {
        $pipeline_success_219 = 0;
    }
    if ($output_219 ne q{} && !defined $output_printed_219) {
        print $output_219;
        if (!($output_219 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_219 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if (do {
{
    my $output_220 = q{};
    my $output_printed_220;
    my $pipeline_success_220 = 1;
    $output_220 .= 'text with pattern in it' . "\n";
if ( !($output_220 =~ m{\n\z}msx) ) { $output_220 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_220_1;
    my @grep_lines_220_1 = split /\n/msx, $output_220;
    my @grep_filtered_220_1 = grep { /pattern/msx } @grep_lines_220_1;
    $grep_result_220_1 = join "\n", @grep_filtered_220_1;
    if (!($grep_result_220_1 =~ m{\n\z}msx || $grep_result_220_1 eq q{})) {
    $grep_result_220_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_220_1 > 0 ? 0 : 1;
    $grep_result_220_1 = q{};
    $output_220 = q{};
    if ((scalar @grep_filtered_220_1) == 0) {
        $pipeline_success_220 = 0;
    }
    if ($output_220 ne q{} && !defined $output_printed_220) {
        print $output_220;
        if (!($output_220 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_220 ) { $main_exit_code = 1; }
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
    my $output_221 = q{};
    my $output_printed_221;
    my $pipeline_success_221 = 1;
        my $grep_result_221_0;
    my @grep_lines_221_0 = ();
    my @grep_filenames_221_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_221_0, $line;
    push @grep_filenames_221_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_221_0 = grep { /pattern/msx } @grep_lines_221_0;
    $grep_result_221_0 = @grep_filtered_221_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_221_0 > 0 ? 0 : 1;
    $output_221 = $grep_result_221_0;
    $output_221 = $grep_result_221_0;

        my $set1_222 = "\\0";
    my $set2_222 = "\\n";
    my $input_222 = $output_221;
    # Expand character ranges for tr command
    my $expanded_set1_222 = $set1_222;
    my $expanded_set2_222 = $set2_222;
    # Handle a-z range in set1
    if ($expanded_set1_222 =~ /a-z/msx) {
    $expanded_set1_222 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_222 =~ /A-Z/msx) {
    $expanded_set1_222 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_222 =~ /\[:upper:\]/msx) {
    $expanded_set1_222 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_222 =~ /\[:lower:\]/msx) {
    $expanded_set1_222 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_222 =~ /a-z/msx) {
    $expanded_set2_222 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_222 =~ /A-Z/msx) {
    $expanded_set2_222 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_222 =~ /\[:upper:\]/msx) {
    $expanded_set2_222 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_222 =~ /\[:lower:\]/msx) {
    $expanded_set2_222 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_221_1 = q{};
    for my $char ( split //msx, $input_222 ) {
    my $pos_222 = index $expanded_set1_222, $char;
    if ( $pos_222 >= 0 && $pos_222 < length $expanded_set2_222 ) {
    $tr_result_221_1 .= substr $expanded_set2_222, $pos_222, 1;
    } else {
    $tr_result_221_1 .= $char;
    }
    }
    if (!($tr_result_221_1 =~ m{\n\z}msx || $tr_result_221_1 eq q{})) {
    $tr_result_221_1 .= "\n";
    }
    $output_221 = $tr_result_221_1;
    $output_221 = $tr_result_221_1;
    if ($output_221 ne q{} && !defined $output_printed_221) {
        print $output_221;
        if (!($output_221 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_221 ) { $main_exit_code = 1; }
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

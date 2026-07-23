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
    my $output_200 = q{};
    my $output_printed_200;
    my $pipeline_success_200 = 1;
    $output_200 .= 'text with pattern' . "\n";
if ( !($output_200 =~ m{\n\z}msx) ) { $output_200 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_200_1;
    my @grep_lines_200_1 = split /\n/msx, $output_200;
    my @grep_filtered_200_1 = grep { /PATTERN/msxi } @grep_lines_200_1;
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
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_201 = q{};
    my $output_printed_201;
    my $pipeline_success_201 = 1;
    $output_201 .= "line1\nline2\nline3";
if ( !($output_201 =~ m{\n\z}msx) ) { $output_201 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_201_1;
    my @grep_lines_201_1 = split /\n/msx, $output_201;
    my @grep_filtered_201_1 = grep { !/line2/msx } @grep_lines_201_1;
    $grep_result_201_1 = join "\n", @grep_filtered_201_1;
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
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_202 = q{};
    my $output_printed_202;
    my $pipeline_success_202 = 1;
    $output_202 .= "match\nno match\nmatch again";
if ( !($output_202 =~ m{\n\z}msx) ) { $output_202 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_202_1;
    my @grep_lines_202_1 = split /\n/msx, $output_202;
    my @grep_filtered_202_1 = grep { /match/msx } @grep_lines_202_1;
    $grep_result_202_1 = scalar @grep_filtered_202_1 . "\n";
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
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
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
    push @grep_with_context_203_1, $grep_lines_203_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_203_1, $grep_lines_203_1[$j];
    }
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
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
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_204_1, $grep_lines_204_1[$j];
    }
    }
    push @grep_with_context_204_1, $grep_lines_204_1[$i];
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_205 = q{};
    my $output_printed_205;
    my $pipeline_success_205 = 1;
    $output_205 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_205 =~ m{\n\z}msx) ) { $output_205 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_205_1;
    my @grep_lines_205_1 = split /\n/msx, $output_205;
    my @grep_filtered_205_1 = grep { /TARGET/msx } @grep_lines_205_1;
    my @grep_with_context_205_1;
    for my $i (0..@grep_lines_205_1-1) {
    if (scalar grep { $_ eq $grep_lines_205_1[$i] } @grep_filtered_205_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_205_1, $grep_lines_205_1[$j];
    }
    }
    push @grep_with_context_205_1, $grep_lines_205_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_205_1, $grep_lines_205_1[$j];
    }
    }
    }
    $grep_result_205_1 = join "\n", @grep_with_context_205_1;
    $CHILD_ERROR = scalar @grep_filtered_205_1 > 0 ? 0 : 1;
    $output_205 = $grep_result_205_1;
    $output_205 = $grep_result_205_1;
    if ((scalar @grep_filtered_205_1) == 0) {
        $pipeline_success_205 = 0;
    }
    if ($output_205 ne q{} && !defined $output_printed_205) {
        print $output_205;
        if (!($output_205 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_205 ) { $main_exit_code = 1; }
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
my @grep_with_filename_206;
for my $line (@grep_filtered_206) {
    push @grep_with_filename_206, "temp_file.txt:$line";
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
$grep_result_207 = join "\n", @grep_filtered_207;
if (!($grep_result_207 =~ m{\n\z}msx || $grep_result_207 eq q{})) {
    $grep_result_207 .= "\n";
}
print $grep_result_207;
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
my @grep_filtered_208 = grep { /content/msx } @grep_lines_208;
$grep_result_208 = @grep_filtered_208 > 0 ? "temp_file.txt" : "";
print $grep_result_208;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_208 > 0 ? 0 : 1;
my $grep_result_209;
my @grep_lines_209 = ();
my @grep_filenames_209 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_209, $line;
        push @grep_filenames_209, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_209 = grep { /nonexistent/msx } @grep_lines_209;
$grep_result_209 = @grep_filtered_209 == 0 ? "temp_file.txt" : "";
print $grep_result_209;
print "\n";
$CHILD_ERROR = $grep_result_209 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
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
    my @grep_matches_211_1;
    foreach my $line (@grep_filtered_211_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_211_1, $1;
    }
    }
    $grep_result_211_1 = join "\n", @grep_matches_211_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
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
    my @grep_with_offset_212_1;
    my $offset_212_1 = 0;
    for my $line (@grep_lines_212_1) {
    if (grep { $_ eq $line } @grep_filtered_212_1) {
    push @grep_with_offset_212_1, sprintf "%d:%s", $offset_212_1, $line;
    }
    $offset_212_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_212_1 = join "\n", @grep_with_offset_212_1;
    if (!($grep_result_212_1 =~ m{\n\z}msx || $grep_result_212_1 eq q{})) {
    $grep_result_212_1 .= "\n";
    }
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
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_213 = q{};
    my $output_printed_213;
    my $pipeline_success_213 = 1;
    $output_213 .= 'text with pattern in it' . "\n";
if ( !($output_213 =~ m{\n\z}msx) ) { $output_213 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_213_1;
    my @grep_lines_213_1 = split /\n/msx, $output_213;
    my @grep_filtered_213_1 = grep { /pattern/msx } @grep_lines_213_1;
    my @grep_numbered_213_1;
    for my $i (0..@grep_lines_213_1-1) {
    if (scalar grep { $_ eq $grep_lines_213_1[$i] } @grep_filtered_213_1) {
    push @grep_numbered_213_1, sprintf "%d:%s", $i + 1, $grep_lines_213_1[$i];
    }
    }
    $grep_result_213_1 = join "\n", @grep_numbered_213_1;
    $CHILD_ERROR = scalar @grep_filtered_213_1 > 0 ? 0 : 1;
    $output_213 = $grep_result_213_1;
    $output_213 = $grep_result_213_1;
    if ((scalar @grep_filtered_213_1) == 0) {
        $pipeline_success_213 = 0;
    }
    if ($output_213 ne q{} && !defined $output_printed_213) {
        print $output_213;
        if (!($output_213 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_213 ) { $main_exit_code = 1; }
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
                if ($file =~ /[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_215 = $find_files_recursive_215->('test_dir', '*');
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
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_216 = $find_files_recursive_216->('test_dir', '*.txt');
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
                if ($file =~ /[.]txt$/msx && $file !~ /.*[.]bak$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
};
my @files_217 = $find_files_recursive_217->('test_dir', '*');
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
my @grep_with_filename_217;
for my $i (0..@grep_lines_217-1) {
    if (scalar grep { $_ eq $grep_lines_217[$i] } @grep_filtered_217) {
        push @grep_with_filename_217, $grep_filenames_217[$i] . ':' . $grep_lines_217[$i];
    }
}
$grep_result_217 = join "\n", @grep_with_filename_217;
if (!($grep_result_217 =~ m{\n\z}msx || $grep_result_217 eq q{})) {
    $grep_result_217 .= "\n";
}
print $grep_result_217;
$CHILD_ERROR = scalar @grep_filtered_217 > 0 ? 0 : 1;
my $grep_result_218;
my @grep_lines_218 = ();
my @grep_filenames_218 = ();
my $find_files_recursive_218;
$find_files_recursive_218 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_218->($path, $pattern));
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
my @files_218 = $find_files_recursive_218->('test_dir', '*.txt');
for my $file (@files_218) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_218, $line;
            push @grep_filenames_218, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_218 = grep { /pattern/msx } @grep_lines_218;
my %file_counts_218;
my @file_order_218;
for my $i (0..@grep_lines_218-1) {
    if (scalar grep { $_ eq $grep_lines_218[$i] } @grep_filtered_218) {
        my $f_218 = $grep_filenames_218[$i];
        push @file_order_218, $f_218 unless exists $file_counts_218{$f_218};
        $file_counts_218{$f_218}++;
    }
}
$grep_result_218 = q{};
for my $file (@file_order_218) {
    $grep_result_218 .= "$file:$file_counts_218{$file}\n";
}
print $grep_result_218;
$CHILD_ERROR = scalar @grep_filtered_218 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_219 = q{};
    my $output_printed_219;
    my $pipeline_success_219 = 1;
        my $grep_result_219_0;
    my @grep_lines_219_0 = ();
    my @grep_filenames_219_0 = ();
    my $find_files_recursive_219_0;
    $find_files_recursive_219_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_219_0->($path, $pattern));
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
    my @files_219_0 = $find_files_recursive_219_0->('test_dir', '*.txt');
    for my $file (@files_219_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_219_0, $line;
    push @grep_filenames_219_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_219_0 = grep { /pattern/msx } @grep_lines_219_0;
    my @grep_with_filename_219_0;
    for my $i (0..@grep_lines_219_0-1) {
    if (scalar grep { $_ eq $grep_lines_219_0[$i] } @grep_filtered_219_0) {
    push @grep_with_filename_219_0, $grep_filenames_219_0[$i] . ':' . $grep_lines_219_0[$i];
    }
    }
    $grep_result_219_0 = join "\n", @grep_with_filename_219_0;
    if (!($grep_result_219_0 =~ m{\n\z}msx || $grep_result_219_0 eq q{})) {
    $grep_result_219_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_219_0 > 0 ? 0 : 1;
    $output_219 = $grep_result_219_0;
    $output_219 = $grep_result_219_0;

        my $output_219_1 = do {
    my $_wc_data = $output_219;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_lines;
    $_wc_result .= "\n";
    $_wc_result;
    };
    $output_219 = $output_219_1;
    if ($output_219 ne q{} && !defined $output_printed_219) {
        print $output_219;
        if (!($output_219 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_219 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_220 = q{};
    my $output_printed_220;
    my $pipeline_success_220 = 1;
    $output_220 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_220 =~ m{\n\z}msx) ) { $output_220 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_220_1;
    my @grep_lines_220_1 = split /\n/msx, $output_220;
    my @grep_filtered_220_1 = grep { /match/msx } @grep_lines_220_1;
    @grep_filtered_220_1 = @grep_filtered_220_1[0..1];
    $grep_result_220_1 = join "\n", @grep_filtered_220_1;
    if (!($grep_result_220_1 =~ m{\n\z}msx || $grep_result_220_1 eq q{})) {
    $grep_result_220_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_220_1 > 0 ? 0 : 1;
    $output_220 = $grep_result_220_1;
    $output_220 = $grep_result_220_1;
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
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if (do {
{
    my $output_221 = q{};
    my $output_printed_221;
    my $pipeline_success_221 = 1;
    $output_221 .= 'text with pattern in it' . "\n";
if ( !($output_221 =~ m{\n\z}msx) ) { $output_221 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_221_1;
    my @grep_lines_221_1 = split /\n/msx, $output_221;
    my @grep_filtered_221_1 = grep { /pattern/msx } @grep_lines_221_1;
    $grep_result_221_1 = join "\n", @grep_filtered_221_1;
    if (!($grep_result_221_1 =~ m{\n\z}msx || $grep_result_221_1 eq q{})) {
    $grep_result_221_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_221_1 > 0 ? 0 : 1;
    $grep_result_221_1 = q{};
    $output_221 = q{};
    if ((scalar @grep_filtered_221_1) == 0) {
        $pipeline_success_221 = 0;
    }
    if ($output_221 ne q{} && !defined $output_printed_221) {
        print $output_221;
        if (!($output_221 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_221 ) { $main_exit_code = 1; }
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
    my $output_222 = q{};
    my $output_printed_222;
    my $pipeline_success_222 = 1;
        my $grep_result_222_0;
    my @grep_lines_222_0 = ();
    my @grep_filenames_222_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_222_0, $line;
    push @grep_filenames_222_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_222_0 = grep { /pattern/msx } @grep_lines_222_0;
    $grep_result_222_0 = @grep_filtered_222_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_222_0 > 0 ? 0 : 1;
    $output_222 = $grep_result_222_0;
    $output_222 = $grep_result_222_0;

        my $set1_223 = "\\0";
    my $set2_223 = "\\n";
    my $input_223 = $output_222;
    # Expand character ranges for tr command
    my $expanded_set1_223 = $set1_223;
    my $expanded_set2_223 = $set2_223;
    # Handle a-z range in set1
    if ($expanded_set1_223 =~ /a-z/msx) {
    $expanded_set1_223 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_223 =~ /A-Z/msx) {
    $expanded_set1_223 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_223 =~ /\[:upper:\]/msx) {
    $expanded_set1_223 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_223 =~ /\[:lower:\]/msx) {
    $expanded_set1_223 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_223 =~ /a-z/msx) {
    $expanded_set2_223 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_223 =~ /A-Z/msx) {
    $expanded_set2_223 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_223 =~ /\[:upper:\]/msx) {
    $expanded_set2_223 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_223 =~ /\[:lower:\]/msx) {
    $expanded_set2_223 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_222_1 = q{};
    for my $char ( split //msx, $input_223 ) {
    my $pos_223 = index $expanded_set1_223, $char;
    if ( $pos_223 >= 0 && $pos_223 < length $expanded_set2_223 ) {
    $tr_result_222_1 .= substr $expanded_set2_223, $pos_223, 1;
    } else {
    $tr_result_222_1 .= $char;
    }
    }
    if (!($tr_result_222_1 =~ m{\n\z}msx || $tr_result_222_1 eq q{})) {
    $tr_result_222_1 .= "\n";
    }
    $output_222 = $tr_result_222_1;
    $output_222 = $tr_result_222_1;
    if ($output_222 ne q{} && !defined $output_printed_222) {
        print $output_222;
        if (!($output_222 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_222 ) { $main_exit_code = 1; }
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

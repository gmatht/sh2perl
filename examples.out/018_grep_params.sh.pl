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
    my $output_191 = q{};
    my $output_printed_191;
    my $pipeline_success_191 = 1;
    $output_191 .= 'text with pattern' . "\n";
if ( !($output_191 =~ m{\n\z}msx) ) { $output_191 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_191_1;
    my @grep_lines_191_1 = split /\n/msx, $output_191;
    my @grep_filtered_191_1 = grep { /PATTERN/msxi } @grep_lines_191_1;
    $grep_result_191_1 = join "\n", @grep_filtered_191_1;
    if (!($grep_result_191_1 =~ m{\n\z}msx || $grep_result_191_1 eq q{})) {
    $grep_result_191_1 .= "\n";
    }
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
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_192 = q{};
    my $output_printed_192;
    my $pipeline_success_192 = 1;
    $output_192 .= "line1\nline2\nline3";
if ( !($output_192 =~ m{\n\z}msx) ) { $output_192 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_192_1;
    my @grep_lines_192_1 = split /\n/msx, $output_192;
    my @grep_filtered_192_1 = grep { !/line2/msx } @grep_lines_192_1;
    $grep_result_192_1 = join "\n", @grep_filtered_192_1;
    if (!($grep_result_192_1 =~ m{\n\z}msx || $grep_result_192_1 eq q{})) {
    $grep_result_192_1 .= "\n";
    }
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
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_193 = q{};
    my $output_printed_193;
    my $pipeline_success_193 = 1;
    $output_193 .= "match\nno match\nmatch again";
if ( !($output_193 =~ m{\n\z}msx) ) { $output_193 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_193_1;
    my @grep_lines_193_1 = split /\n/msx, $output_193;
    my @grep_filtered_193_1 = grep { /match/msx } @grep_lines_193_1;
    $grep_result_193_1 = scalar @grep_filtered_193_1 . "\n";
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
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
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
    push @grep_with_context_194_1, $grep_lines_194_1[$i];
    for my $j (($i + 1)..($i + 2)) {
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_195 = q{};
    my $output_printed_195;
    my $pipeline_success_195 = 1;
    $output_195 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_195 =~ m{\n\z}msx) ) { $output_195 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_195_1;
    my @grep_lines_195_1 = split /\n/msx, $output_195;
    my @grep_filtered_195_1 = grep { /TARGET/msx } @grep_lines_195_1;
    my @grep_with_context_195_1;
    for my $i (0..@grep_lines_195_1-1) {
    if (scalar grep { $_ eq $grep_lines_195_1[$i] } @grep_filtered_195_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_195_1, $grep_lines_195_1[$j];
    }
    }
    push @grep_with_context_195_1, $grep_lines_195_1[$i];
    }
    }
    $grep_result_195_1 = join "\n", @grep_with_context_195_1;
    $CHILD_ERROR = scalar @grep_filtered_195_1 > 0 ? 0 : 1;
    $output_195 = $grep_result_195_1;
    $output_195 = $grep_result_195_1;
    if ((scalar @grep_filtered_195_1) == 0) {
        $pipeline_success_195 = 0;
    }
    if ($output_195 ne q{} && !defined $output_printed_195) {
        print $output_195;
        if (!($output_195 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_195 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_196 = q{};
    my $output_printed_196;
    my $pipeline_success_196 = 1;
    $output_196 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_196 =~ m{\n\z}msx) ) { $output_196 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_196_1;
    my @grep_lines_196_1 = split /\n/msx, $output_196;
    my @grep_filtered_196_1 = grep { /TARGET/msx } @grep_lines_196_1;
    my @grep_with_context_196_1;
    for my $i (0..@grep_lines_196_1-1) {
    if (scalar grep { $_ eq $grep_lines_196_1[$i] } @grep_filtered_196_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_196_1, $grep_lines_196_1[$j];
    }
    }
    push @grep_with_context_196_1, $grep_lines_196_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_196_1, $grep_lines_196_1[$j];
    }
    }
    }
    $grep_result_196_1 = join "\n", @grep_with_context_196_1;
    $CHILD_ERROR = scalar @grep_filtered_196_1 > 0 ? 0 : 1;
    $output_196 = $grep_result_196_1;
    $output_196 = $grep_result_196_1;
    if ((scalar @grep_filtered_196_1) == 0) {
        $pipeline_success_196 = 0;
    }
    if ($output_196 ne q{} && !defined $output_printed_196) {
        print $output_196;
        if (!($output_196 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_196 ) { $main_exit_code = 1; }
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
my @grep_with_filename_197;
for my $line (@grep_filtered_197) {
    push @grep_with_filename_197, "temp_file.txt:$line";
}
$grep_result_197 = join "\n", @grep_with_filename_197;
if (!($grep_result_197 =~ m{\n\z}msx || $grep_result_197 eq q{})) {
    $grep_result_197 .= "\n";
}
print $grep_result_197;
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
my @grep_filtered_198 = grep { /content/msx } @grep_lines_198;
$grep_result_198 = join "\n", @grep_filtered_198;
if (!($grep_result_198 =~ m{\n\z}msx || $grep_result_198 eq q{})) {
    $grep_result_198 .= "\n";
}
print $grep_result_198;
$CHILD_ERROR = scalar @grep_filtered_198 > 0 ? 0 : 1;
my $grep_result_199;
my @grep_lines_199 = ();
my @grep_filenames_199 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_199, $line;
        push @grep_filenames_199, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_199 = grep { /content/msx } @grep_lines_199;
$grep_result_199 = @grep_filtered_199 > 0 ? "temp_file.txt" : "";
print $grep_result_199;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_199 > 0 ? 0 : 1;
my $grep_result_200;
my @grep_lines_200 = ();
my @grep_filenames_200 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_200, $line;
        push @grep_filenames_200, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_200 = grep { /nonexistent/msx } @grep_lines_200;
$grep_result_200 = @grep_filtered_200 == 0 ? "temp_file.txt" : "";
print $grep_result_200;
print "\n";
$CHILD_ERROR = $grep_result_200 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
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
    my @grep_matches_202_1;
    foreach my $line (@grep_filtered_202_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_202_1, $1;
    }
    }
    $grep_result_202_1 = join "\n", @grep_matches_202_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_203 = q{};
    my $output_printed_203;
    my $pipeline_success_203 = 1;
    $output_203 .= 'text with pattern in it' . "\n";
if ( !($output_203 =~ m{\n\z}msx) ) { $output_203 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_203_1;
    my @grep_lines_203_1 = split /\n/msx, $output_203;
    my @grep_filtered_203_1 = grep { /pattern/msx } @grep_lines_203_1;
    my @grep_with_offset_203_1;
    my $offset_203_1 = 0;
    for my $line (@grep_lines_203_1) {
    if (grep { $_ eq $line } @grep_filtered_203_1) {
    push @grep_with_offset_203_1, sprintf "%d:%s", $offset_203_1, $line;
    }
    $offset_203_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_203_1 = join "\n", @grep_with_offset_203_1;
    if (!($grep_result_203_1 =~ m{\n\z}msx || $grep_result_203_1 eq q{})) {
    $grep_result_203_1 .= "\n";
    }
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
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_204 = q{};
    my $output_printed_204;
    my $pipeline_success_204 = 1;
    $output_204 .= 'text with pattern in it' . "\n";
if ( !($output_204 =~ m{\n\z}msx) ) { $output_204 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_204_1;
    my @grep_lines_204_1 = split /\n/msx, $output_204;
    my @grep_filtered_204_1 = grep { /pattern/msx } @grep_lines_204_1;
    my @grep_numbered_204_1;
    for my $i (0..@grep_lines_204_1-1) {
    if (scalar grep { $_ eq $grep_lines_204_1[$i] } @grep_filtered_204_1) {
    push @grep_numbered_204_1, sprintf "%d:%s", $i + 1, $grep_lines_204_1[$i];
    }
    }
    $grep_result_204_1 = join "\n", @grep_numbered_204_1;
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
                if ($file =~ /[.]txt$/msx) {
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
my @grep_with_filename_207;
for my $i (0..@grep_lines_207-1) {
    if (scalar grep { $_ eq $grep_lines_207[$i] } @grep_filtered_207) {
        push @grep_with_filename_207, $grep_filenames_207[$i] . ':' . $grep_lines_207[$i];
    }
}
$grep_result_207 = join "\n", @grep_with_filename_207;
if (!($grep_result_207 =~ m{\n\z}msx || $grep_result_207 eq q{})) {
    $grep_result_207 .= "\n";
}
print $grep_result_207;
$CHILD_ERROR = scalar @grep_filtered_207 > 0 ? 0 : 1;
my $grep_result_208;
my @grep_lines_208 = ();
my @grep_filenames_208 = ();
my $find_files_recursive_208;
$find_files_recursive_208 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_208->($path, $pattern));
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
my @files_208 = $find_files_recursive_208->('test_dir', '*');
for my $file (@files_208) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_208, $line;
            push @grep_filenames_208, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_208 = grep { /pattern/msx } @grep_lines_208;
my @grep_with_filename_208;
for my $i (0..@grep_lines_208-1) {
    if (scalar grep { $_ eq $grep_lines_208[$i] } @grep_filtered_208) {
        push @grep_with_filename_208, $grep_filenames_208[$i] . ':' . $grep_lines_208[$i];
    }
}
$grep_result_208 = join "\n", @grep_with_filename_208;
if (!($grep_result_208 =~ m{\n\z}msx || $grep_result_208 eq q{})) {
    $grep_result_208 .= "\n";
}
print $grep_result_208;
$CHILD_ERROR = scalar @grep_filtered_208 > 0 ? 0 : 1;
my $grep_result_209;
my @grep_lines_209 = ();
my @grep_filenames_209 = ();
my $find_files_recursive_209;
$find_files_recursive_209 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_209->($path, $pattern));
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
my @files_209 = $find_files_recursive_209->('test_dir', '*.txt');
for my $file (@files_209) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_209, $line;
            push @grep_filenames_209, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_209 = grep { /pattern/msx } @grep_lines_209;
my %file_counts_209;
my @file_order_209;
for my $i (0..@grep_lines_209-1) {
    if (scalar grep { $_ eq $grep_lines_209[$i] } @grep_filtered_209) {
        my $f_209 = $grep_filenames_209[$i];
        push @file_order_209, $f_209 unless exists $file_counts_209{$f_209};
        $file_counts_209{$f_209}++;
    }
}
$grep_result_209 = q{};
for my $file (@file_order_209) {
    $grep_result_209 .= "$file:$file_counts_209{$file}\n";
}
print $grep_result_209;
$CHILD_ERROR = scalar @grep_filtered_209 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_210 = q{};
    my $output_printed_210;
    my $pipeline_success_210 = 1;
        my $grep_result_210_0;
    my @grep_lines_210_0 = ();
    my @grep_filenames_210_0 = ();
    my $find_files_recursive_210_0;
    $find_files_recursive_210_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_210_0->($path, $pattern));
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
    my @files_210_0 = $find_files_recursive_210_0->('test_dir', '*.txt');
    for my $file (@files_210_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_210_0, $line;
    push @grep_filenames_210_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_210_0 = grep { /pattern/msx } @grep_lines_210_0;
    my @grep_with_filename_210_0;
    for my $i (0..@grep_lines_210_0-1) {
    if (scalar grep { $_ eq $grep_lines_210_0[$i] } @grep_filtered_210_0) {
    push @grep_with_filename_210_0, $grep_filenames_210_0[$i] . ':' . $grep_lines_210_0[$i];
    }
    }
    $grep_result_210_0 = join "\n", @grep_with_filename_210_0;
    if (!($grep_result_210_0 =~ m{\n\z}msx || $grep_result_210_0 eq q{})) {
    $grep_result_210_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_210_0 > 0 ? 0 : 1;
    $output_210 = $grep_result_210_0;
    $output_210 = $grep_result_210_0;

        my $output_210_1 = do {
    my $_wc_data = $output_210;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_lines;
    $_wc_result .= "\n";
    $_wc_result;
    };
    $output_210 = $output_210_1;
    if ($output_210 ne q{} && !defined $output_printed_210) {
        print $output_210;
        if (!($output_210 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_210 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_211 = q{};
    my $output_printed_211;
    my $pipeline_success_211 = 1;
    $output_211 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_211 =~ m{\n\z}msx) ) { $output_211 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_211_1;
    my @grep_lines_211_1 = split /\n/msx, $output_211;
    my @grep_filtered_211_1 = grep { /match/msx } @grep_lines_211_1;
    @grep_filtered_211_1 = @grep_filtered_211_1[0..1];
    $grep_result_211_1 = join "\n", @grep_filtered_211_1;
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
if (do {
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
    $grep_result_212_1 = join "\n", @grep_filtered_212_1;
    if (!($grep_result_212_1 =~ m{\n\z}msx || $grep_result_212_1 eq q{})) {
    $grep_result_212_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_212_1 > 0 ? 0 : 1;
    $grep_result_212_1 = q{};
    $output_212 = q{};
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
    my $output_213 = q{};
    my $output_printed_213;
    my $pipeline_success_213 = 1;
        my $grep_result_213_0;
    my @grep_lines_213_0 = ();
    my @grep_filenames_213_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_213_0, $line;
    push @grep_filenames_213_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_213_0 = grep { /pattern/msx } @grep_lines_213_0;
    $grep_result_213_0 = @grep_filtered_213_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_213_0 > 0 ? 0 : 1;
    $output_213 = $grep_result_213_0;
    $output_213 = $grep_result_213_0;

        my $set1_214 = "\\0";
    my $set2_214 = "\\n";
    my $input_214 = $output_213;
    # Expand character ranges for tr command
    my $expanded_set1_214 = $set1_214;
    my $expanded_set2_214 = $set2_214;
    # Handle a-z range in set1
    if ($expanded_set1_214 =~ /a-z/msx) {
    $expanded_set1_214 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_214 =~ /A-Z/msx) {
    $expanded_set1_214 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set1
    if ($expanded_set1_214 =~ /\[:upper:\]/msx) {
    $expanded_set1_214 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set1
    if ($expanded_set1_214 =~ /\[:lower:\]/msx) {
    $expanded_set1_214 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_214 =~ /a-z/msx) {
    $expanded_set2_214 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_214 =~ /A-Z/msx) {
    $expanded_set2_214 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:upper:] POSIX class in set2
    if ($expanded_set2_214 =~ /\[:upper:\]/msx) {
    $expanded_set2_214 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle [:lower:] POSIX class in set2
    if ($expanded_set2_214 =~ /\[:lower:\]/msx) {
    $expanded_set2_214 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
    }
    my $tr_result_213_1 = q{};
    for my $char ( split //msx, $input_214 ) {
    my $pos_214 = index $expanded_set1_214, $char;
    if ( $pos_214 >= 0 && $pos_214 < length $expanded_set2_214 ) {
    $tr_result_213_1 .= substr $expanded_set2_214, $pos_214, 1;
    } else {
    $tr_result_213_1 .= $char;
    }
    }
    if (!($tr_result_213_1 =~ m{\n\z}msx || $tr_result_213_1 eq q{})) {
    $tr_result_213_1 .= "\n";
    }
    $output_213 = $tr_result_213_1;
    $output_213 = $tr_result_213_1;
    if ($output_213 ne q{} && !defined $output_printed_213) {
        print $output_213;
        if (!($output_213 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_213 ) { $main_exit_code = 1; }
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

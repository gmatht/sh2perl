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
our $CHILD_ERROR;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== Basic grep parameters ==\n";
# Original bash: echo "text with pattern" | grep -i "PATTERN"
{
    my $output_195 = q{};
    my $output_printed_195;
    my $pipeline_success_195 = 1;
    $output_195 .= 'text with pattern' . "\n";
if ( !($output_195 =~ m{\n\z}msx) ) { $output_195 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_195_1;
    my @grep_lines_195_1 = split /\n/msx, $output_195;
    my @grep_filtered_195_1 = grep { /PATTERN/msxi } @grep_lines_195_1;
    $grep_result_195_1 = join "\n", @grep_filtered_195_1;
    if (!($grep_result_195_1 =~ m{\n\z}msx || $grep_result_195_1 eq q{})) {
    $grep_result_195_1 .= "\n";
    }
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
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_196 = q{};
    my $output_printed_196;
    my $pipeline_success_196 = 1;
    $output_196 .= "line1\nline2\nline3";
if ( !($output_196 =~ m{\n\z}msx) ) { $output_196 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_196_1;
    my @grep_lines_196_1 = split /\n/msx, $output_196;
    my @grep_filtered_196_1 = grep { !/line2/msx } @grep_lines_196_1;
    $grep_result_196_1 = join "\n", @grep_filtered_196_1;
    if (!($grep_result_196_1 =~ m{\n\z}msx || $grep_result_196_1 eq q{})) {
    $grep_result_196_1 .= "\n";
    }
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
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_197 = q{};
    my $output_printed_197;
    my $pipeline_success_197 = 1;
    $output_197 .= "match\nno match\nmatch again";
if ( !($output_197 =~ m{\n\z}msx) ) { $output_197 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_197_1;
    my @grep_lines_197_1 = split /\n/msx, $output_197;
    my @grep_filtered_197_1 = grep { /match/msx } @grep_lines_197_1;
    $grep_result_197_1 = scalar @grep_filtered_197_1;
    $CHILD_ERROR = scalar @grep_filtered_197_1 > 0 ? 0 : 1;
    $output_197 = $grep_result_197_1;
    $output_197 = $grep_result_197_1;
    if ((scalar @grep_filtered_197_1) == 0) {
        $pipeline_success_197 = 0;
    }
    if ($output_197 ne q{} && !defined $output_printed_197) {
        print $output_197;
        if (!($output_197 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_197 ) { $main_exit_code = 1; }
    }
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_198 = q{};
    my $output_printed_198;
    my $pipeline_success_198 = 1;
    $output_198 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_198 =~ m{\n\z}msx) ) { $output_198 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_198_1;
    my @grep_lines_198_1 = split /\n/msx, $output_198;
    my @grep_filtered_198_1 = grep { /TARGET/msx } @grep_lines_198_1;
    my @grep_with_context_198_1;
    for my $i (0..@grep_lines_198_1-1) {
    if (scalar grep { $_ eq $grep_lines_198_1[$i] } @grep_filtered_198_1) {
    push @grep_with_context_198_1, $grep_lines_198_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_198_1, $grep_lines_198_1[$j];
    }
    }
    }
    $grep_result_198_1 = join "\n", @grep_with_context_198_1;
    $CHILD_ERROR = scalar @grep_filtered_198_1 > 0 ? 0 : 1;
    $output_198 = $grep_result_198_1;
    $output_198 = $grep_result_198_1;
    if ((scalar @grep_filtered_198_1) == 0) {
        $pipeline_success_198 = 0;
    }
    if ($output_198 ne q{} && !defined $output_printed_198) {
        print $output_198;
        if (!($output_198 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_198 ) { $main_exit_code = 1; }
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_199 = q{};
    my $output_printed_199;
    my $pipeline_success_199 = 1;
    $output_199 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_199 =~ m{\n\z}msx) ) { $output_199 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_199_1;
    my @grep_lines_199_1 = split /\n/msx, $output_199;
    my @grep_filtered_199_1 = grep { /TARGET/msx } @grep_lines_199_1;
    my @grep_with_context_199_1;
    for my $i (0..@grep_lines_199_1-1) {
    if (scalar grep { $_ eq $grep_lines_199_1[$i] } @grep_filtered_199_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_199_1, $grep_lines_199_1[$j];
    }
    }
    push @grep_with_context_199_1, $grep_lines_199_1[$i];
    }
    }
    $grep_result_199_1 = join "\n", @grep_with_context_199_1;
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
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_200 = q{};
    my $output_printed_200;
    my $pipeline_success_200 = 1;
    $output_200 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_200 =~ m{\n\z}msx) ) { $output_200 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_200_1;
    my @grep_lines_200_1 = split /\n/msx, $output_200;
    my @grep_filtered_200_1 = grep { /TARGET/msx } @grep_lines_200_1;
    my @grep_with_context_200_1;
    for my $i (0..@grep_lines_200_1-1) {
    if (scalar grep { $_ eq $grep_lines_200_1[$i] } @grep_filtered_200_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_200_1, $grep_lines_200_1[$j];
    }
    }
    push @grep_with_context_200_1, $grep_lines_200_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_200_1, $grep_lines_200_1[$j];
    }
    }
    }
    $grep_result_200_1 = join "\n", @grep_with_context_200_1;
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
my $grep_result_201;
my @grep_lines_201 = ();
my @grep_filenames_201 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_201, $line;
        push @grep_filenames_201, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_201 = grep { /content/msx } @grep_lines_201;
my @grep_with_filename_201;
for my $line (@grep_filtered_201) {
    push @grep_with_filename_201, "temp_file.txt:$line";
}
$grep_result_201 = join "\n", @grep_with_filename_201;
if (!($grep_result_201 =~ m{\n\z}msx || $grep_result_201 eq q{})) {
    $grep_result_201 .= "\n";
}
print $grep_result_201;
$CHILD_ERROR = scalar @grep_filtered_201 > 0 ? 0 : 1;
my $grep_result_202;
my @grep_lines_202 = ();
my @grep_filenames_202 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_202, $line;
        push @grep_filenames_202, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_202 = grep { /content/msx } @grep_lines_202;
$grep_result_202 = join "\n", @grep_filtered_202;
if (!($grep_result_202 =~ m{\n\z}msx || $grep_result_202 eq q{})) {
    $grep_result_202 .= "\n";
}
print $grep_result_202;
$CHILD_ERROR = scalar @grep_filtered_202 > 0 ? 0 : 1;
my $grep_result_203;
my @grep_lines_203 = ();
my @grep_filenames_203 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_203, $line;
        push @grep_filenames_203, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_203 = grep { /content/msx } @grep_lines_203;
$grep_result_203 = @grep_filtered_203 > 0 ? "temp_file.txt" : "";
print $grep_result_203;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_203 > 0 ? 0 : 1;
my $grep_result_204;
my @grep_lines_204 = ();
my @grep_filenames_204 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_204, $line;
        push @grep_filenames_204, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_204 = grep { /nonexistent/msx } @grep_lines_204;
$grep_result_204 = @grep_filtered_204 == 0 ? "temp_file.txt" : "";
print $grep_result_204;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_204 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
{
    my $output_206 = q{};
    my $output_printed_206;
    my $pipeline_success_206 = 1;
    $output_206 .= 'text with pattern in it' . "\n";
if ( !($output_206 =~ m{\n\z}msx) ) { $output_206 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_206_1;
    my @grep_lines_206_1 = split /\n/msx, $output_206;
    my @grep_filtered_206_1 = grep { /pattern/msx } @grep_lines_206_1;
    my @grep_matches_206_1;
    foreach my $line (@grep_filtered_206_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_206_1, $1;
    }
    }
    $grep_result_206_1 = join "\n", @grep_matches_206_1;
    $CHILD_ERROR = scalar @grep_filtered_206_1 > 0 ? 0 : 1;
    $output_206 = $grep_result_206_1;
    $output_206 = $grep_result_206_1;
    if ((scalar @grep_filtered_206_1) == 0) {
        $pipeline_success_206 = 0;
    }
    if ($output_206 ne q{} && !defined $output_printed_206) {
        print $output_206;
        if (!($output_206 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_206 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_207 = q{};
    my $output_printed_207;
    my $pipeline_success_207 = 1;
    $output_207 .= 'text with pattern in it' . "\n";
if ( !($output_207 =~ m{\n\z}msx) ) { $output_207 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_207_1;
    my @grep_lines_207_1 = split /\n/msx, $output_207;
    my @grep_filtered_207_1 = grep { /pattern/msx } @grep_lines_207_1;
    my @grep_with_offset_207_1;
    my $offset_207_1 = 0;
    for my $line (@grep_lines_207_1) {
    if (grep { $_ eq $line } @grep_filtered_207_1) {
    push @grep_with_offset_207_1, sprintf "%d:%s", $offset_207_1, $line;
    }
    $offset_207_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_207_1 = join "\n", @grep_with_offset_207_1;
    if (!($grep_result_207_1 =~ m{\n\z}msx || $grep_result_207_1 eq q{})) {
    $grep_result_207_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_207_1 > 0 ? 0 : 1;
    $output_207 = $grep_result_207_1;
    $output_207 = $grep_result_207_1;
    if ((scalar @grep_filtered_207_1) == 0) {
        $pipeline_success_207 = 0;
    }
    if ($output_207 ne q{} && !defined $output_printed_207) {
        print $output_207;
        if (!($output_207 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_207 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_208 = q{};
    my $output_printed_208;
    my $pipeline_success_208 = 1;
    $output_208 .= 'text with pattern in it' . "\n";
if ( !($output_208 =~ m{\n\z}msx) ) { $output_208 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_208_1;
    my @grep_lines_208_1 = split /\n/msx, $output_208;
    my @grep_filtered_208_1 = grep { /pattern/msx } @grep_lines_208_1;
    my @grep_numbered_208_1;
    for my $i (0..@grep_lines_208_1-1) {
    if (scalar grep { $_ eq $grep_lines_208_1[$i] } @grep_filtered_208_1) {
    push @grep_numbered_208_1, sprintf "%d:%s", $i + 1, $grep_lines_208_1[$i];
    }
    }
    $grep_result_208_1 = join "\n", @grep_numbered_208_1;
    $CHILD_ERROR = scalar @grep_filtered_208_1 > 0 ? 0 : 1;
    $output_208 = $grep_result_208_1;
    $output_208 = $grep_result_208_1;
    if ((scalar @grep_filtered_208_1) == 0) {
        $pipeline_success_208 = 0;
    }
    if ($output_208 ne q{} && !defined $output_printed_208) {
        print $output_208;
        if (!($output_208 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_208 ) { $main_exit_code = 1; }
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
my $grep_result_210;
my @grep_lines_210 = ();
my @grep_filenames_210 = ();
sub find_files_recursive_210 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_210($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
}
my @files_210 = find_files_recursive_210('test_dir', '*');
for my $file (@files_210) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_210, $line;
            push @grep_filenames_210, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_210 = grep { /pattern/msx } @grep_lines_210;
my @grep_with_filename_210;
for my $i (0..@grep_lines_210-1) {
    if (scalar grep { $_ eq $grep_lines_210[$i] } @grep_filtered_210) {
        push @grep_with_filename_210, "$grep_filenames_210[$i]:$grep_lines_210[$i]";
    }
}
$grep_result_210 = join "\n", @grep_with_filename_210;
if (!($grep_result_210 =~ m{\n\z}msx || $grep_result_210 eq q{})) {
    $grep_result_210 .= "\n";
}
print $grep_result_210;
$CHILD_ERROR = scalar @grep_filtered_210 > 0 ? 0 : 1;
my $grep_result_211;
my @grep_lines_211 = ();
my @grep_filenames_211 = ();
sub find_files_recursive_211 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_211($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
}
my @files_211 = find_files_recursive_211('test_dir', '*.txt');
for my $file (@files_211) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_211, $line;
            push @grep_filenames_211, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_211 = grep { /pattern/msx } @grep_lines_211;
my @grep_with_filename_211;
for my $i (0..@grep_lines_211-1) {
    if (scalar grep { $_ eq $grep_lines_211[$i] } @grep_filtered_211) {
        push @grep_with_filename_211, "$grep_filenames_211[$i]:$grep_lines_211[$i]";
    }
}
$grep_result_211 = join "\n", @grep_with_filename_211;
if (!($grep_result_211 =~ m{\n\z}msx || $grep_result_211 eq q{})) {
    $grep_result_211 .= "\n";
}
print $grep_result_211;
$CHILD_ERROR = scalar @grep_filtered_211 > 0 ? 0 : 1;
my $grep_result_212;
my @grep_lines_212 = ();
my @grep_filenames_212 = ();
sub find_files_recursive_212 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_212($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /[.]txt$/msx && $file !~ /.*[.]bak$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
}
my @files_212 = find_files_recursive_212('test_dir', '*');
for my $file (@files_212) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_212, $line;
            push @grep_filenames_212, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_212 = grep { /pattern/msx } @grep_lines_212;
my @grep_with_filename_212;
for my $i (0..@grep_lines_212-1) {
    if (scalar grep { $_ eq $grep_lines_212[$i] } @grep_filtered_212) {
        push @grep_with_filename_212, "$grep_filenames_212[$i]:$grep_lines_212[$i]";
    }
}
$grep_result_212 = join "\n", @grep_with_filename_212;
if (!($grep_result_212 =~ m{\n\z}msx || $grep_result_212 eq q{})) {
    $grep_result_212 .= "\n";
}
print $grep_result_212;
$CHILD_ERROR = scalar @grep_filtered_212 > 0 ? 0 : 1;
my $grep_result_213;
my @grep_lines_213 = ();
my @grep_filenames_213 = ();
sub find_files_recursive_213 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_213($path, $pattern));
            } elsif (-f $path) {
                if ($file =~ /.*[.]txt$/msx) {
                    push @files, $path;
                }
            }
        }
        closedir $dh;
    }
    return @files;
}
my @files_213 = find_files_recursive_213('test_dir', '*.txt');
for my $file (@files_213) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_213, $line;
            push @grep_filenames_213, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_213 = grep { /pattern/msx } @grep_lines_213;
my %file_counts_213;
my @file_order_213;
for my $i (0..@grep_lines_213-1) {
    if (scalar grep { $_ eq $grep_lines_213[$i] } @grep_filtered_213) {
        my $f_213 = $grep_filenames_213[$i];
        push @file_order_213, $f_213 unless exists $file_counts_213{$f_213};
        $file_counts_213{$f_213}++;
    }
}
$grep_result_213 = q{};
for my $file (@file_order_213) {
    $grep_result_213 .= "$file:$file_counts_213{$file}\n";
}
$grep_result_213 =~ s/\n$//msx; # Remove trailing newline
print $grep_result_213;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_213 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_214 = q{};
    my $output_printed_214;
    my $pipeline_success_214 = 1;
        my $grep_result_214_0;
    my @grep_lines_214_0 = ();
    my @grep_filenames_214_0 = ();
    sub find_files_recursive_214_0 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, find_files_recursive_214_0($path, $pattern));
    } elsif (-f $path) {
    if ($file =~ /.*[.]txt$/msx) {
    push @files, $path;
    }
    }
    }
    closedir $dh;
    }
    return @files;
    }
    my @files_214_0 = find_files_recursive_214_0('test_dir', '*.txt');
    for my $file (@files_214_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_214_0, $line;
    push @grep_filenames_214_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_214_0 = grep { /pattern/msx } @grep_lines_214_0;
    my @grep_with_filename_214_0;
    for my $i (0..@grep_lines_214_0-1) {
    if (scalar grep { $_ eq $grep_lines_214_0[$i] } @grep_filtered_214_0) {
    push @grep_with_filename_214_0, "$grep_filenames_214_0[$i]:$grep_lines_214_0[$i]";
    }
    }
    $grep_result_214_0 = join "\n", @grep_with_filename_214_0;
    if (!($grep_result_214_0 =~ m{\n\z}msx || $grep_result_214_0 eq q{})) {
    $grep_result_214_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_214_0 > 0 ? 0 : 1;
    $output_214 = $grep_result_214_0;
    $output_214 = $grep_result_214_0;
    if ((scalar @grep_filtered_214_0) == 0) {
        $pipeline_success_214 = 0;
    }

        use IPC::Open3;
    my @wc_args_214_1 = ('-l');
    my ($wc_in_214_1, $wc_out_214_1, $wc_err_214_1);
    my $wc_pid_214_1 = open3($wc_in_214_1, $wc_out_214_1, $wc_err_214_1, 'wc', @wc_args_214_1);
    print {$wc_in_214_1} $output_214;
    close $wc_in_214_1 or die "Close failed: $OS_ERROR\n";
    my $output_214_1 = do { local $/ = undef; <$wc_out_214_1> };
    if ($output_214_1 eq q{}) { $output_214_1 = "0\n"; }
    close $wc_out_214_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_214_1, 0;
    $output_214 = $output_214_1;
    if ($output_214 ne q{} && !defined $output_printed_214) {
        print $output_214;
        if (!($output_214 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_214 ) { $main_exit_code = 1; }
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_215 = q{};
    my $output_printed_215;
    my $pipeline_success_215 = 1;
    $output_215 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_215 =~ m{\n\z}msx) ) { $output_215 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_215_1;
    my @grep_lines_215_1 = split /\n/msx, $output_215;
    my @grep_filtered_215_1 = grep { /match/msx } @grep_lines_215_1;
    @grep_filtered_215_1 = @grep_filtered_215_1[0..1];
    $grep_result_215_1 = join "\n", @grep_filtered_215_1;
    if (!($grep_result_215_1 =~ m{\n\z}msx || $grep_result_215_1 eq q{})) {
    $grep_result_215_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_215_1 > 0 ? 0 : 1;
    $output_215 = $grep_result_215_1;
    $output_215 = $grep_result_215_1;
    if ((scalar @grep_filtered_215_1) == 0) {
        $pipeline_success_215 = 0;
    }
    if ($output_215 ne q{} && !defined $output_printed_215) {
        print $output_215;
        if (!($output_215 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_215 ) { $main_exit_code = 1; }
    }
if (do {
{
    my $output_216 = q{};
    my $output_printed_216;
    my $pipeline_success_216 = 1;
    $output_216 .= 'text with pattern in it' . "\n";
if ( !($output_216 =~ m{\n\z}msx) ) { $output_216 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_216_1;
    my @grep_lines_216_1 = split /\n/msx, $output_216;
    my @grep_filtered_216_1 = grep { /pattern/msx } @grep_lines_216_1;
    $grep_result_216_1 = join "\n", @grep_filtered_216_1;
    if (!($grep_result_216_1 =~ m{\n\z}msx || $grep_result_216_1 eq q{})) {
    $grep_result_216_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_216_1 > 0 ? 0 : 1;
    $output_216 = q{};
    if ((scalar @grep_filtered_216_1) == 0) {
        $pipeline_success_216 = 0;
    }
    if ($output_216 ne q{} && !defined $output_printed_216) {
        print $output_216;
        if (!($output_216 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_216 ) { $main_exit_code = 1; }
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
    my $output_217 = q{};
    my $output_printed_217;
    my $pipeline_success_217 = 1;
        my $grep_result_217_0;
    my @grep_lines_217_0 = ();
    my @grep_filenames_217_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_217_0, $line;
    push @grep_filenames_217_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_217_0 = grep { /pattern/msx } @grep_lines_217_0;
    $grep_result_217_0 = @grep_filtered_217_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_217_0 > 0 ? 0 : 1;
    $output_217 = $grep_result_217_0;
    $output_217 = $grep_result_217_0;
    if ((scalar @grep_filtered_217_0) == 0) {
        $pipeline_success_217 = 0;
    }

        my $set1_218 = "\\0";
    my $set2_218 = "\\n";
    my $input_218 = $output_217;
    # Expand character ranges for tr command
    my $expanded_set1_218 = $set1_218;
    my $expanded_set2_218 = $set2_218;
    # Handle a-z range in set1
    if ($expanded_set1_218 =~ /a-z/msx) {
    $expanded_set1_218 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_218 =~ /A-Z/msx) {
    $expanded_set1_218 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_218 =~ /a-z/msx) {
    $expanded_set2_218 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_218 =~ /A-Z/msx) {
    $expanded_set2_218 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_217_1 = q{};
    for my $char ( split //msx, $input_218 ) {
    my $pos_218 = index $expanded_set1_218, $char;
    if ( $pos_218 >= 0 && $pos_218 < length $expanded_set2_218 ) {
    $tr_result_217_1 .= substr $expanded_set2_218, $pos_218, 1;
    } else {
    $tr_result_217_1 .= $char;
    }
    }
    if (!($tr_result_217_1 =~ m{\n\z}msx || $tr_result_217_1 eq q{})) {
    $tr_result_217_1 .= "\n";
    }
    $output_217 = $tr_result_217_1;
    $output_217 = $tr_result_217_1;
    if ($output_217 ne q{} && !defined $output_printed_217) {
        print $output_217;
        if (!($output_217 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_217 ) { $main_exit_code = 1; }
    }
if ( -e "temp_file.txt" ) {
    if ( -d "temp_file.txt" ) {
        carp "rm: carping: ", "temp_file.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "temp_file.txt" ) {
            $main_exit_code = 0;
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
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_dir" ) {
            $main_exit_code = 0;
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

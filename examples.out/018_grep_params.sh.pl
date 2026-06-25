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
    my $output_47 = q{};
    my $output_printed_47;
    my $pipeline_success_47 = 1;
    $output_47 .= 'text with pattern' . "\n";
if ( !($output_47 =~ m{\n\z}msx) ) { $output_47 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_47_1;
    my @grep_lines_47_1 = split /\n/msx, $output_47;
    my @grep_filtered_47_1 = grep { /PATTERN/msxi } @grep_lines_47_1;
    $grep_result_47_1 = join "\n", @grep_filtered_47_1;
    if (!($grep_result_47_1 =~ m{\n\z}msx || $grep_result_47_1 eq q{})) {
    $grep_result_47_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_47_1 > 0 ? 0 : 1;
    $output_47 = $grep_result_47_1;
    $output_47 = $grep_result_47_1;
    if ((scalar @grep_filtered_47_1) == 0) {
        $pipeline_success_47 = 0;
    }
    if ($output_47 ne q{} && !defined $output_printed_47) {
        print $output_47;
        if (!($output_47 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_47 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_48 = q{};
    my $output_printed_48;
    my $pipeline_success_48 = 1;
    $output_48 .= "line1\nline2\nline3";
if ( !($output_48 =~ m{\n\z}msx) ) { $output_48 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_48_1;
    my @grep_lines_48_1 = split /\n/msx, $output_48;
    my @grep_filtered_48_1 = grep { !/line2/msx } @grep_lines_48_1;
    $grep_result_48_1 = join "\n", @grep_filtered_48_1;
    if (!($grep_result_48_1 =~ m{\n\z}msx || $grep_result_48_1 eq q{})) {
    $grep_result_48_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_48_1 > 0 ? 0 : 1;
    $output_48 = $grep_result_48_1;
    $output_48 = $grep_result_48_1;
    if ((scalar @grep_filtered_48_1) == 0) {
        $pipeline_success_48 = 0;
    }
    if ($output_48 ne q{} && !defined $output_printed_48) {
        print $output_48;
        if (!($output_48 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_48 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_49 = q{};
    my $output_printed_49;
    my $pipeline_success_49 = 1;
    $output_49 .= "match\nno match\nmatch again";
if ( !($output_49 =~ m{\n\z}msx) ) { $output_49 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_49_1;
    my @grep_lines_49_1 = split /\n/msx, $output_49;
    my @grep_filtered_49_1 = grep { /match/msx } @grep_lines_49_1;
    $grep_result_49_1 = scalar @grep_filtered_49_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_49_1 > 0 ? 0 : 1;
    $output_49 = $grep_result_49_1;
    $output_49 = $grep_result_49_1;
    if ((scalar @grep_filtered_49_1) == 0) {
        $pipeline_success_49 = 0;
    }
    if ($output_49 ne q{} && !defined $output_printed_49) {
        print $output_49;
        if (!($output_49 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_49 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_50 = q{};
    my $output_printed_50;
    my $pipeline_success_50 = 1;
    $output_50 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_50 =~ m{\n\z}msx) ) { $output_50 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_50_1;
    my @grep_lines_50_1 = split /\n/msx, $output_50;
    my @grep_filtered_50_1 = grep { /TARGET/msx } @grep_lines_50_1;
    my @grep_with_context_50_1;
    for my $i (0..@grep_lines_50_1-1) {
    if (scalar grep { $_ eq $grep_lines_50_1[$i] } @grep_filtered_50_1) {
    push @grep_with_context_50_1, $grep_lines_50_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_50_1, $grep_lines_50_1[$j];
    }
    }
    }
    $grep_result_50_1 = join "\n", @grep_with_context_50_1;
    $CHILD_ERROR = scalar @grep_filtered_50_1 > 0 ? 0 : 1;
    $output_50 = $grep_result_50_1;
    $output_50 = $grep_result_50_1;
    if ((scalar @grep_filtered_50_1) == 0) {
        $pipeline_success_50 = 0;
    }
    if ($output_50 ne q{} && !defined $output_printed_50) {
        print $output_50;
        if (!($output_50 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_50 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_51 = q{};
    my $output_printed_51;
    my $pipeline_success_51 = 1;
    $output_51 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_51 =~ m{\n\z}msx) ) { $output_51 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_51_1;
    my @grep_lines_51_1 = split /\n/msx, $output_51;
    my @grep_filtered_51_1 = grep { /TARGET/msx } @grep_lines_51_1;
    my @grep_with_context_51_1;
    for my $i (0..@grep_lines_51_1-1) {
    if (scalar grep { $_ eq $grep_lines_51_1[$i] } @grep_filtered_51_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_51_1, $grep_lines_51_1[$j];
    }
    }
    push @grep_with_context_51_1, $grep_lines_51_1[$i];
    }
    }
    $grep_result_51_1 = join "\n", @grep_with_context_51_1;
    $CHILD_ERROR = scalar @grep_filtered_51_1 > 0 ? 0 : 1;
    $output_51 = $grep_result_51_1;
    $output_51 = $grep_result_51_1;
    if ((scalar @grep_filtered_51_1) == 0) {
        $pipeline_success_51 = 0;
    }
    if ($output_51 ne q{} && !defined $output_printed_51) {
        print $output_51;
        if (!($output_51 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_51 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_52 = q{};
    my $output_printed_52;
    my $pipeline_success_52 = 1;
    $output_52 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_52 =~ m{\n\z}msx) ) { $output_52 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_52_1;
    my @grep_lines_52_1 = split /\n/msx, $output_52;
    my @grep_filtered_52_1 = grep { /TARGET/msx } @grep_lines_52_1;
    my @grep_with_context_52_1;
    for my $i (0..@grep_lines_52_1-1) {
    if (scalar grep { $_ eq $grep_lines_52_1[$i] } @grep_filtered_52_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_52_1, $grep_lines_52_1[$j];
    }
    }
    push @grep_with_context_52_1, $grep_lines_52_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_52_1, $grep_lines_52_1[$j];
    }
    }
    }
    $grep_result_52_1 = join "\n", @grep_with_context_52_1;
    $CHILD_ERROR = scalar @grep_filtered_52_1 > 0 ? 0 : 1;
    $output_52 = $grep_result_52_1;
    $output_52 = $grep_result_52_1;
    if ((scalar @grep_filtered_52_1) == 0) {
        $pipeline_success_52 = 0;
    }
    if ($output_52 ne q{} && !defined $output_printed_52) {
        print $output_52;
        if (!($output_52 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_52 ) { $main_exit_code = 1; }
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
my $grep_result_53;
my @grep_lines_53 = ();
my @grep_filenames_53 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_53, $line;
        push @grep_filenames_53, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_53 = grep { /content/msx } @grep_lines_53;
my @grep_with_filename_53;
for my $line (@grep_filtered_53) {
    push @grep_with_filename_53, "temp_file.txt:$line";
}
$grep_result_53 = join "\n", @grep_with_filename_53;
if (!($grep_result_53 =~ m{\n\z}msx || $grep_result_53 eq q{})) {
    $grep_result_53 .= "\n";
}
print $grep_result_53;
$CHILD_ERROR = scalar @grep_filtered_53 > 0 ? 0 : 1;
my $grep_result_54;
my @grep_lines_54 = ();
my @grep_filenames_54 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_54, $line;
        push @grep_filenames_54, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_54 = grep { /content/msx } @grep_lines_54;
$grep_result_54 = join "\n", @grep_filtered_54;
if (!($grep_result_54 =~ m{\n\z}msx || $grep_result_54 eq q{})) {
    $grep_result_54 .= "\n";
}
print $grep_result_54;
$CHILD_ERROR = scalar @grep_filtered_54 > 0 ? 0 : 1;
my $grep_result_55;
my @grep_lines_55 = ();
my @grep_filenames_55 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_55, $line;
        push @grep_filenames_55, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_55 = grep { /content/msx } @grep_lines_55;
$grep_result_55 = @grep_filtered_55 > 0 ? "temp_file.txt" : "";
print $grep_result_55;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_55 > 0 ? 0 : 1;
my $grep_result_56;
my @grep_lines_56 = ();
my @grep_filenames_56 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_56, $line;
        push @grep_filenames_56, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_56 = grep { /nonexistent/msx } @grep_lines_56;
$grep_result_56 = @grep_filtered_56 == 0 ? "temp_file.txt" : "";
print $grep_result_56;
print "\n";
$CHILD_ERROR = $grep_result_56 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
{
    my $output_58 = q{};
    my $output_printed_58;
    my $pipeline_success_58 = 1;
    $output_58 .= 'text with pattern in it' . "\n";
if ( !($output_58 =~ m{\n\z}msx) ) { $output_58 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_58_1;
    my @grep_lines_58_1 = split /\n/msx, $output_58;
    my @grep_filtered_58_1 = grep { /pattern/msx } @grep_lines_58_1;
    my @grep_matches_58_1;
    foreach my $line (@grep_filtered_58_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_58_1, $1;
    }
    }
    $grep_result_58_1 = join "\n", @grep_matches_58_1;
    $CHILD_ERROR = scalar @grep_filtered_58_1 > 0 ? 0 : 1;
    $output_58 = $grep_result_58_1;
    $output_58 = $grep_result_58_1;
    if ((scalar @grep_filtered_58_1) == 0) {
        $pipeline_success_58 = 0;
    }
    if ($output_58 ne q{} && !defined $output_printed_58) {
        print $output_58;
        if (!($output_58 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_58 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_59 = q{};
    my $output_printed_59;
    my $pipeline_success_59 = 1;
    $output_59 .= 'text with pattern in it' . "\n";
if ( !($output_59 =~ m{\n\z}msx) ) { $output_59 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_59_1;
    my @grep_lines_59_1 = split /\n/msx, $output_59;
    my @grep_filtered_59_1 = grep { /pattern/msx } @grep_lines_59_1;
    my @grep_with_offset_59_1;
    my $offset_59_1 = 0;
    for my $line (@grep_lines_59_1) {
    if (grep { $_ eq $line } @grep_filtered_59_1) {
    push @grep_with_offset_59_1, sprintf "%d:%s", $offset_59_1, $line;
    }
    $offset_59_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_59_1 = join "\n", @grep_with_offset_59_1;
    if (!($grep_result_59_1 =~ m{\n\z}msx || $grep_result_59_1 eq q{})) {
    $grep_result_59_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_59_1 > 0 ? 0 : 1;
    $output_59 = $grep_result_59_1;
    $output_59 = $grep_result_59_1;
    if ((scalar @grep_filtered_59_1) == 0) {
        $pipeline_success_59 = 0;
    }
    if ($output_59 ne q{} && !defined $output_printed_59) {
        print $output_59;
        if (!($output_59 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_59 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_60 = q{};
    my $output_printed_60;
    my $pipeline_success_60 = 1;
    $output_60 .= 'text with pattern in it' . "\n";
if ( !($output_60 =~ m{\n\z}msx) ) { $output_60 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_60_1;
    my @grep_lines_60_1 = split /\n/msx, $output_60;
    my @grep_filtered_60_1 = grep { /pattern/msx } @grep_lines_60_1;
    my @grep_numbered_60_1;
    for my $i (0..@grep_lines_60_1-1) {
    if (scalar grep { $_ eq $grep_lines_60_1[$i] } @grep_filtered_60_1) {
    push @grep_numbered_60_1, sprintf "%d:%s", $i + 1, $grep_lines_60_1[$i];
    }
    }
    $grep_result_60_1 = join "\n", @grep_numbered_60_1;
    $CHILD_ERROR = scalar @grep_filtered_60_1 > 0 ? 0 : 1;
    $output_60 = $grep_result_60_1;
    $output_60 = $grep_result_60_1;
    if ((scalar @grep_filtered_60_1) == 0) {
        $pipeline_success_60 = 0;
    }
    if ($output_60 ne q{} && !defined $output_printed_60) {
        print $output_60;
        if (!($output_60 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_60 ) { $main_exit_code = 1; }
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
my $grep_result_62;
my @grep_lines_62 = ();
my @grep_filenames_62 = ();
my $find_files_recursive_62;
$find_files_recursive_62 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_62->($path, $pattern));
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
my @files_62 = $find_files_recursive_62->('test_dir', '*');
for my $file (@files_62) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_62, $line;
            push @grep_filenames_62, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_62 = grep { /pattern/msx } @grep_lines_62;
my @grep_with_filename_62;
for my $i (0..@grep_lines_62-1) {
    if (scalar grep { $_ eq $grep_lines_62[$i] } @grep_filtered_62) {
        push @grep_with_filename_62, $grep_filenames_62[$i] . ':' . $grep_lines_62[$i];
    }
}
$grep_result_62 = join "\n", @grep_with_filename_62;
if (!($grep_result_62 =~ m{\n\z}msx || $grep_result_62 eq q{})) {
    $grep_result_62 .= "\n";
}
print $grep_result_62;
$CHILD_ERROR = scalar @grep_filtered_62 > 0 ? 0 : 1;
my $grep_result_63;
my @grep_lines_63 = ();
my @grep_filenames_63 = ();
my $find_files_recursive_63;
$find_files_recursive_63 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_63->($path, $pattern));
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
my @files_63 = $find_files_recursive_63->('test_dir', '*.txt');
for my $file (@files_63) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_63, $line;
            push @grep_filenames_63, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_63 = grep { /pattern/msx } @grep_lines_63;
my @grep_with_filename_63;
for my $i (0..@grep_lines_63-1) {
    if (scalar grep { $_ eq $grep_lines_63[$i] } @grep_filtered_63) {
        push @grep_with_filename_63, $grep_filenames_63[$i] . ':' . $grep_lines_63[$i];
    }
}
$grep_result_63 = join "\n", @grep_with_filename_63;
if (!($grep_result_63 =~ m{\n\z}msx || $grep_result_63 eq q{})) {
    $grep_result_63 .= "\n";
}
print $grep_result_63;
$CHILD_ERROR = scalar @grep_filtered_63 > 0 ? 0 : 1;
my $grep_result_64;
my @grep_lines_64 = ();
my @grep_filenames_64 = ();
my $find_files_recursive_64;
$find_files_recursive_64 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_64->($path, $pattern));
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
my @files_64 = $find_files_recursive_64->('test_dir', '*');
for my $file (@files_64) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_64, $line;
            push @grep_filenames_64, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_64 = grep { /pattern/msx } @grep_lines_64;
my @grep_with_filename_64;
for my $i (0..@grep_lines_64-1) {
    if (scalar grep { $_ eq $grep_lines_64[$i] } @grep_filtered_64) {
        push @grep_with_filename_64, $grep_filenames_64[$i] . ':' . $grep_lines_64[$i];
    }
}
$grep_result_64 = join "\n", @grep_with_filename_64;
if (!($grep_result_64 =~ m{\n\z}msx || $grep_result_64 eq q{})) {
    $grep_result_64 .= "\n";
}
print $grep_result_64;
$CHILD_ERROR = scalar @grep_filtered_64 > 0 ? 0 : 1;
my $grep_result_65;
my @grep_lines_65 = ();
my @grep_filenames_65 = ();
my $find_files_recursive_65;
$find_files_recursive_65 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_65->($path, $pattern));
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
my @files_65 = $find_files_recursive_65->('test_dir', '*.txt');
for my $file (@files_65) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_65, $line;
            push @grep_filenames_65, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_65 = grep { /pattern/msx } @grep_lines_65;
my %file_counts_65;
my @file_order_65;
for my $i (0..@grep_lines_65-1) {
    if (scalar grep { $_ eq $grep_lines_65[$i] } @grep_filtered_65) {
        my $f_65 = $grep_filenames_65[$i];
        push @file_order_65, $f_65 unless exists $file_counts_65{$f_65};
        $file_counts_65{$f_65}++;
    }
}
$grep_result_65 = q{};
for my $file (@file_order_65) {
    $grep_result_65 .= "$file:$file_counts_65{$file}\n";
}
print $grep_result_65;
$CHILD_ERROR = scalar @grep_filtered_65 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_66 = q{};
    my $output_printed_66;
    my $pipeline_success_66 = 1;
        my $grep_result_66_0;
    my @grep_lines_66_0 = ();
    my @grep_filenames_66_0 = ();
    my $find_files_recursive_66_0;
    $find_files_recursive_66_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_66_0->($path, $pattern));
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
    my @files_66_0 = $find_files_recursive_66_0->('test_dir', '*.txt');
    for my $file (@files_66_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_66_0, $line;
    push @grep_filenames_66_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_66_0 = grep { /pattern/msx } @grep_lines_66_0;
    my @grep_with_filename_66_0;
    for my $i (0..@grep_lines_66_0-1) {
    if (scalar grep { $_ eq $grep_lines_66_0[$i] } @grep_filtered_66_0) {
    push @grep_with_filename_66_0, $grep_filenames_66_0[$i] . ':' . $grep_lines_66_0[$i];
    }
    }
    $grep_result_66_0 = join "\n", @grep_with_filename_66_0;
    if (!($grep_result_66_0 =~ m{\n\z}msx || $grep_result_66_0 eq q{})) {
    $grep_result_66_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_66_0 > 0 ? 0 : 1;
    $output_66 = $grep_result_66_0;
    $output_66 = $grep_result_66_0;

        use IPC::Open3;
    my @wc_args_66_1 = ('-l');
    my ($wc_in_66_1, $wc_out_66_1, $wc_err_66_1);
    my $wc_pid_66_1 = open3($wc_in_66_1, $wc_out_66_1, $wc_err_66_1, 'wc', @wc_args_66_1);
    print {$wc_in_66_1} $output_66;
    close $wc_in_66_1 or die "Close failed: $OS_ERROR\n";
    my $output_66_1 = do { local $/ = undef; <$wc_out_66_1> };
    if ($output_66_1 eq q{}) { $output_66_1 = "0\n"; }
    close $wc_out_66_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_66_1, 0;
    $output_66 = $output_66_1;
    if ($output_66 ne q{} && !defined $output_printed_66) {
        print $output_66;
        if (!($output_66 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_66 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_67 = q{};
    my $output_printed_67;
    my $pipeline_success_67 = 1;
    $output_67 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_67 =~ m{\n\z}msx) ) { $output_67 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_67_1;
    my @grep_lines_67_1 = split /\n/msx, $output_67;
    my @grep_filtered_67_1 = grep { /match/msx } @grep_lines_67_1;
    @grep_filtered_67_1 = @grep_filtered_67_1[0..1];
    $grep_result_67_1 = join "\n", @grep_filtered_67_1;
    if (!($grep_result_67_1 =~ m{\n\z}msx || $grep_result_67_1 eq q{})) {
    $grep_result_67_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_67_1 > 0 ? 0 : 1;
    $output_67 = $grep_result_67_1;
    $output_67 = $grep_result_67_1;
    if ((scalar @grep_filtered_67_1) == 0) {
        $pipeline_success_67 = 0;
    }
    if ($output_67 ne q{} && !defined $output_printed_67) {
        print $output_67;
        if (!($output_67 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_67 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if (do {
{
    my $output_68 = q{};
    my $output_printed_68;
    my $pipeline_success_68 = 1;
    $output_68 .= 'text with pattern in it' . "\n";
if ( !($output_68 =~ m{\n\z}msx) ) { $output_68 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_68_1;
    my @grep_lines_68_1 = split /\n/msx, $output_68;
    my @grep_filtered_68_1 = grep { /pattern/msx } @grep_lines_68_1;
    $grep_result_68_1 = join "\n", @grep_filtered_68_1;
    if (!($grep_result_68_1 =~ m{\n\z}msx || $grep_result_68_1 eq q{})) {
    $grep_result_68_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_68_1 > 0 ? 0 : 1;
    $grep_result_68_1 = q{};
    $output_68 = q{};
    if ((scalar @grep_filtered_68_1) == 0) {
        $pipeline_success_68 = 0;
    }
    if ($output_68 ne q{} && !defined $output_printed_68) {
        print $output_68;
        if (!($output_68 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_68 ) { $main_exit_code = 1; }
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
    my $output_69 = q{};
    my $output_printed_69;
    my $pipeline_success_69 = 1;
        my $grep_result_69_0;
    my @grep_lines_69_0 = ();
    my @grep_filenames_69_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_69_0, $line;
    push @grep_filenames_69_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_69_0 = grep { /pattern/msx } @grep_lines_69_0;
    $grep_result_69_0 = @grep_filtered_69_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_69_0 > 0 ? 0 : 1;
    $output_69 = $grep_result_69_0;
    $output_69 = $grep_result_69_0;

        my $set1_70 = "\\0";
    my $set2_70 = "\\n";
    my $input_70 = $output_69;
    # Expand character ranges for tr command
    my $expanded_set1_70 = $set1_70;
    my $expanded_set2_70 = $set2_70;
    # Handle a-z range in set1
    if ($expanded_set1_70 =~ /a-z/msx) {
    $expanded_set1_70 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_70 =~ /A-Z/msx) {
    $expanded_set1_70 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_70 =~ /a-z/msx) {
    $expanded_set2_70 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_70 =~ /A-Z/msx) {
    $expanded_set2_70 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_69_1 = q{};
    for my $char ( split //msx, $input_70 ) {
    my $pos_70 = index $expanded_set1_70, $char;
    if ( $pos_70 >= 0 && $pos_70 < length $expanded_set2_70 ) {
    $tr_result_69_1 .= substr $expanded_set2_70, $pos_70, 1;
    } else {
    $tr_result_69_1 .= $char;
    }
    }
    if (!($tr_result_69_1 =~ m{\n\z}msx || $tr_result_69_1 eq q{})) {
    $tr_result_69_1 .= "\n";
    }
    $output_69 = $tr_result_69_1;
    $output_69 = $tr_result_69_1;
    if ($output_69 ne q{} && !defined $output_printed_69) {
        print $output_69;
        if (!($output_69 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_69 ) { $main_exit_code = 1; }
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

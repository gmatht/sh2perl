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
    my $output_178 = q{};
    my $output_printed_178;
    my $pipeline_success_178 = 1;
    $output_178 .= 'text with pattern' . "\n";
if ( !($output_178 =~ m{\n\z}msx) ) { $output_178 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_178_1;
    my @grep_lines_178_1 = split /\n/msx, $output_178;
    my @grep_filtered_178_1 = grep { /PATTERN/msxi } @grep_lines_178_1;
    $grep_result_178_1 = join "\n", @grep_filtered_178_1;
    if (!($grep_result_178_1 =~ m{\n\z}msx || $grep_result_178_1 eq q{})) {
    $grep_result_178_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_178_1 > 0 ? 0 : 1;
    $output_178 = $grep_result_178_1;
    $output_178 = $grep_result_178_1;
    if ((scalar @grep_filtered_178_1) == 0) {
        $pipeline_success_178 = 0;
    }
    if ($output_178 ne q{} && !defined $output_printed_178) {
        print $output_178;
        if (!($output_178 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_178 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_179 = q{};
    my $output_printed_179;
    my $pipeline_success_179 = 1;
    $output_179 .= "line1\nline2\nline3";
if ( !($output_179 =~ m{\n\z}msx) ) { $output_179 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_179_1;
    my @grep_lines_179_1 = split /\n/msx, $output_179;
    my @grep_filtered_179_1 = grep { !/line2/msx } @grep_lines_179_1;
    $grep_result_179_1 = join "\n", @grep_filtered_179_1;
    if (!($grep_result_179_1 =~ m{\n\z}msx || $grep_result_179_1 eq q{})) {
    $grep_result_179_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_179_1 > 0 ? 0 : 1;
    $output_179 = $grep_result_179_1;
    $output_179 = $grep_result_179_1;
    if ((scalar @grep_filtered_179_1) == 0) {
        $pipeline_success_179 = 0;
    }
    if ($output_179 ne q{} && !defined $output_printed_179) {
        print $output_179;
        if (!($output_179 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_179 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_180 = q{};
    my $output_printed_180;
    my $pipeline_success_180 = 1;
    $output_180 .= "match\nno match\nmatch again";
if ( !($output_180 =~ m{\n\z}msx) ) { $output_180 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_180_1;
    my @grep_lines_180_1 = split /\n/msx, $output_180;
    my @grep_filtered_180_1 = grep { /match/msx } @grep_lines_180_1;
    $grep_result_180_1 = scalar @grep_filtered_180_1 . "\n";
    $CHILD_ERROR = scalar @grep_filtered_180_1 > 0 ? 0 : 1;
    $output_180 = $grep_result_180_1;
    $output_180 = $grep_result_180_1;
    if ((scalar @grep_filtered_180_1) == 0) {
        $pipeline_success_180 = 0;
    }
    if ($output_180 ne q{} && !defined $output_printed_180) {
        print $output_180;
        if (!($output_180 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_180 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_181 = q{};
    my $output_printed_181;
    my $pipeline_success_181 = 1;
    $output_181 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_181 =~ m{\n\z}msx) ) { $output_181 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_181_1;
    my @grep_lines_181_1 = split /\n/msx, $output_181;
    my @grep_filtered_181_1 = grep { /TARGET/msx } @grep_lines_181_1;
    my @grep_with_context_181_1;
    for my $i (0..@grep_lines_181_1-1) {
    if (scalar grep { $_ eq $grep_lines_181_1[$i] } @grep_filtered_181_1) {
    push @grep_with_context_181_1, $grep_lines_181_1[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_181_1, $grep_lines_181_1[$j];
    }
    }
    }
    $grep_result_181_1 = join "\n", @grep_with_context_181_1;
    $CHILD_ERROR = scalar @grep_filtered_181_1 > 0 ? 0 : 1;
    $output_181 = $grep_result_181_1;
    $output_181 = $grep_result_181_1;
    if ((scalar @grep_filtered_181_1) == 0) {
        $pipeline_success_181 = 0;
    }
    if ($output_181 ne q{} && !defined $output_printed_181) {
        print $output_181;
        if (!($output_181 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_181 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_182 = q{};
    my $output_printed_182;
    my $pipeline_success_182 = 1;
    $output_182 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_182 =~ m{\n\z}msx) ) { $output_182 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_182_1;
    my @grep_lines_182_1 = split /\n/msx, $output_182;
    my @grep_filtered_182_1 = grep { /TARGET/msx } @grep_lines_182_1;
    my @grep_with_context_182_1;
    for my $i (0..@grep_lines_182_1-1) {
    if (scalar grep { $_ eq $grep_lines_182_1[$i] } @grep_filtered_182_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_182_1, $grep_lines_182_1[$j];
    }
    }
    push @grep_with_context_182_1, $grep_lines_182_1[$i];
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
        if (!($output_182 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_182 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_183 = q{};
    my $output_printed_183;
    my $pipeline_success_183 = 1;
    $output_183 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_183 =~ m{\n\z}msx) ) { $output_183 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_183_1;
    my @grep_lines_183_1 = split /\n/msx, $output_183;
    my @grep_filtered_183_1 = grep { /TARGET/msx } @grep_lines_183_1;
    my @grep_with_context_183_1;
    for my $i (0..@grep_lines_183_1-1) {
    if (scalar grep { $_ eq $grep_lines_183_1[$i] } @grep_filtered_183_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_183_1, $grep_lines_183_1[$j];
    }
    }
    push @grep_with_context_183_1, $grep_lines_183_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_183_1, $grep_lines_183_1[$j];
    }
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
        if (!($output_183 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_183 ) { $main_exit_code = 1; }
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
my $grep_result_184;
my @grep_lines_184 = ();
my @grep_filenames_184 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_184, $line;
        push @grep_filenames_184, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_184 = grep { /content/msx } @grep_lines_184;
my @grep_with_filename_184;
for my $line (@grep_filtered_184) {
    push @grep_with_filename_184, "temp_file.txt:$line";
}
$grep_result_184 = join "\n", @grep_with_filename_184;
if (!($grep_result_184 =~ m{\n\z}msx || $grep_result_184 eq q{})) {
    $grep_result_184 .= "\n";
}
print $grep_result_184;
$CHILD_ERROR = scalar @grep_filtered_184 > 0 ? 0 : 1;
my $grep_result_185;
my @grep_lines_185 = ();
my @grep_filenames_185 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_185, $line;
        push @grep_filenames_185, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_185 = grep { /content/msx } @grep_lines_185;
$grep_result_185 = join "\n", @grep_filtered_185;
if (!($grep_result_185 =~ m{\n\z}msx || $grep_result_185 eq q{})) {
    $grep_result_185 .= "\n";
}
print $grep_result_185;
$CHILD_ERROR = scalar @grep_filtered_185 > 0 ? 0 : 1;
my $grep_result_186;
my @grep_lines_186 = ();
my @grep_filenames_186 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_186, $line;
        push @grep_filenames_186, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_186 = grep { /content/msx } @grep_lines_186;
$grep_result_186 = @grep_filtered_186 > 0 ? "temp_file.txt" : "";
print $grep_result_186;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_186 > 0 ? 0 : 1;
my $grep_result_187;
my @grep_lines_187 = ();
my @grep_filenames_187 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_187, $line;
        push @grep_filenames_187, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_187 = grep { /nonexistent/msx } @grep_lines_187;
$grep_result_187 = @grep_filtered_187 == 0 ? "temp_file.txt" : "";
print $grep_result_187;
print "\n";
$CHILD_ERROR = $grep_result_187 ne q{} ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
{
    my $output_189 = q{};
    my $output_printed_189;
    my $pipeline_success_189 = 1;
    $output_189 .= 'text with pattern in it' . "\n";
if ( !($output_189 =~ m{\n\z}msx) ) { $output_189 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_189_1;
    my @grep_lines_189_1 = split /\n/msx, $output_189;
    my @grep_filtered_189_1 = grep { /pattern/msx } @grep_lines_189_1;
    my @grep_matches_189_1;
    foreach my $line (@grep_filtered_189_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_189_1, $1;
    }
    }
    $grep_result_189_1 = join "\n", @grep_matches_189_1;
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
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_190 = q{};
    my $output_printed_190;
    my $pipeline_success_190 = 1;
    $output_190 .= 'text with pattern in it' . "\n";
if ( !($output_190 =~ m{\n\z}msx) ) { $output_190 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_190_1;
    my @grep_lines_190_1 = split /\n/msx, $output_190;
    my @grep_filtered_190_1 = grep { /pattern/msx } @grep_lines_190_1;
    my @grep_with_offset_190_1;
    my $offset_190_1 = 0;
    for my $line (@grep_lines_190_1) {
    if (grep { $_ eq $line } @grep_filtered_190_1) {
    push @grep_with_offset_190_1, sprintf "%d:%s", $offset_190_1, $line;
    }
    $offset_190_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_190_1 = join "\n", @grep_with_offset_190_1;
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
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_191 = q{};
    my $output_printed_191;
    my $pipeline_success_191 = 1;
    $output_191 .= 'text with pattern in it' . "\n";
if ( !($output_191 =~ m{\n\z}msx) ) { $output_191 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_191_1;
    my @grep_lines_191_1 = split /\n/msx, $output_191;
    my @grep_filtered_191_1 = grep { /pattern/msx } @grep_lines_191_1;
    my @grep_numbered_191_1;
    for my $i (0..@grep_lines_191_1-1) {
    if (scalar grep { $_ eq $grep_lines_191_1[$i] } @grep_filtered_191_1) {
    push @grep_numbered_191_1, sprintf "%d:%s", $i + 1, $grep_lines_191_1[$i];
    }
    }
    $grep_result_191_1 = join "\n", @grep_numbered_191_1;
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
my $grep_result_193;
my @grep_lines_193 = ();
my @grep_filenames_193 = ();
my $find_files_recursive_193;
$find_files_recursive_193 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_193->($path, $pattern));
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
my @files_193 = $find_files_recursive_193->('test_dir', '*');
for my $file (@files_193) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_193, $line;
            push @grep_filenames_193, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_193 = grep { /pattern/msx } @grep_lines_193;
my @grep_with_filename_193;
for my $i (0..@grep_lines_193-1) {
    if (scalar grep { $_ eq $grep_lines_193[$i] } @grep_filtered_193) {
        push @grep_with_filename_193, $grep_filenames_193[$i] . ':' . $grep_lines_193[$i];
    }
}
$grep_result_193 = join "\n", @grep_with_filename_193;
if (!($grep_result_193 =~ m{\n\z}msx || $grep_result_193 eq q{})) {
    $grep_result_193 .= "\n";
}
print $grep_result_193;
$CHILD_ERROR = scalar @grep_filtered_193 > 0 ? 0 : 1;
my $grep_result_194;
my @grep_lines_194 = ();
my @grep_filenames_194 = ();
my $find_files_recursive_194;
$find_files_recursive_194 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_194->($path, $pattern));
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
my @files_194 = $find_files_recursive_194->('test_dir', '*.txt');
for my $file (@files_194) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_194, $line;
            push @grep_filenames_194, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_194 = grep { /pattern/msx } @grep_lines_194;
my @grep_with_filename_194;
for my $i (0..@grep_lines_194-1) {
    if (scalar grep { $_ eq $grep_lines_194[$i] } @grep_filtered_194) {
        push @grep_with_filename_194, $grep_filenames_194[$i] . ':' . $grep_lines_194[$i];
    }
}
$grep_result_194 = join "\n", @grep_with_filename_194;
if (!($grep_result_194 =~ m{\n\z}msx || $grep_result_194 eq q{})) {
    $grep_result_194 .= "\n";
}
print $grep_result_194;
$CHILD_ERROR = scalar @grep_filtered_194 > 0 ? 0 : 1;
my $grep_result_195;
my @grep_lines_195 = ();
my @grep_filenames_195 = ();
my $find_files_recursive_195;
$find_files_recursive_195 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_195->($path, $pattern));
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
my @files_195 = $find_files_recursive_195->('test_dir', '*');
for my $file (@files_195) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_195, $line;
            push @grep_filenames_195, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_195 = grep { /pattern/msx } @grep_lines_195;
my @grep_with_filename_195;
for my $i (0..@grep_lines_195-1) {
    if (scalar grep { $_ eq $grep_lines_195[$i] } @grep_filtered_195) {
        push @grep_with_filename_195, $grep_filenames_195[$i] . ':' . $grep_lines_195[$i];
    }
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
my $find_files_recursive_196;
$find_files_recursive_196 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, $find_files_recursive_196->($path, $pattern));
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
my @files_196 = $find_files_recursive_196->('test_dir', '*.txt');
for my $file (@files_196) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_196, $line;
            push @grep_filenames_196, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_196 = grep { /pattern/msx } @grep_lines_196;
my %file_counts_196;
my @file_order_196;
for my $i (0..@grep_lines_196-1) {
    if (scalar grep { $_ eq $grep_lines_196[$i] } @grep_filtered_196) {
        my $f_196 = $grep_filenames_196[$i];
        push @file_order_196, $f_196 unless exists $file_counts_196{$f_196};
        $file_counts_196{$f_196}++;
    }
}
$grep_result_196 = q{};
for my $file (@file_order_196) {
    $grep_result_196 .= "$file:$file_counts_196{$file}\n";
}
print $grep_result_196;
$CHILD_ERROR = scalar @grep_filtered_196 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_197 = q{};
    my $output_printed_197;
    my $pipeline_success_197 = 1;
        my $grep_result_197_0;
    my @grep_lines_197_0 = ();
    my @grep_filenames_197_0 = ();
    my $find_files_recursive_197_0;
    $find_files_recursive_197_0 = sub {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, $find_files_recursive_197_0->($path, $pattern));
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
    my @files_197_0 = $find_files_recursive_197_0->('test_dir', '*.txt');
    for my $file (@files_197_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_197_0, $line;
    push @grep_filenames_197_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_197_0 = grep { /pattern/msx } @grep_lines_197_0;
    my @grep_with_filename_197_0;
    for my $i (0..@grep_lines_197_0-1) {
    if (scalar grep { $_ eq $grep_lines_197_0[$i] } @grep_filtered_197_0) {
    push @grep_with_filename_197_0, $grep_filenames_197_0[$i] . ':' . $grep_lines_197_0[$i];
    }
    }
    $grep_result_197_0 = join "\n", @grep_with_filename_197_0;
    if (!($grep_result_197_0 =~ m{\n\z}msx || $grep_result_197_0 eq q{})) {
    $grep_result_197_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_197_0 > 0 ? 0 : 1;
    $output_197 = $grep_result_197_0;
    $output_197 = $grep_result_197_0;

        use IPC::Open3;
    my @wc_args_197_1 = ('-l');
    my ($wc_in_197_1, $wc_out_197_1, $wc_err_197_1);
    my $wc_pid_197_1 = open3($wc_in_197_1, $wc_out_197_1, $wc_err_197_1, 'wc', @wc_args_197_1);
    print {$wc_in_197_1} $output_197;
    close $wc_in_197_1 or die "Close failed: $OS_ERROR\n";
    my $output_197_1 = do { local $/ = undef; <$wc_out_197_1> };
    if ($output_197_1 eq q{}) { $output_197_1 = "0\n"; }
    close $wc_out_197_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_197_1, 0;
    $output_197 = $output_197_1;
    if ($output_197 ne q{} && !defined $output_printed_197) {
        print $output_197;
        if (!($output_197 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_197 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_198 = q{};
    my $output_printed_198;
    my $pipeline_success_198 = 1;
    $output_198 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_198 =~ m{\n\z}msx) ) { $output_198 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_198_1;
    my @grep_lines_198_1 = split /\n/msx, $output_198;
    my @grep_filtered_198_1 = grep { /match/msx } @grep_lines_198_1;
    @grep_filtered_198_1 = @grep_filtered_198_1[0..1];
    $grep_result_198_1 = join "\n", @grep_filtered_198_1;
    if (!($grep_result_198_1 =~ m{\n\z}msx || $grep_result_198_1 eq q{})) {
    $grep_result_198_1 .= "\n";
    }
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
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
if (do {
{
    my $output_199 = q{};
    my $output_printed_199;
    my $pipeline_success_199 = 1;
    $output_199 .= 'text with pattern in it' . "\n";
if ( !($output_199 =~ m{\n\z}msx) ) { $output_199 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_199_1;
    my @grep_lines_199_1 = split /\n/msx, $output_199;
    my @grep_filtered_199_1 = grep { /pattern/msx } @grep_lines_199_1;
    $grep_result_199_1 = join "\n", @grep_filtered_199_1;
    if (!($grep_result_199_1 =~ m{\n\z}msx || $grep_result_199_1 eq q{})) {
    $grep_result_199_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_199_1 > 0 ? 0 : 1;
    $grep_result_199_1 = q{};
    $output_199 = q{};
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
    $CHILD_ERROR == 0
}) {
        print "found\n";
}
if ($CHILD_ERROR != 0) {
        print "not found\n";
}
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_200 = q{};
    my $output_printed_200;
    my $pipeline_success_200 = 1;
        my $grep_result_200_0;
    my @grep_lines_200_0 = ();
    my @grep_filenames_200_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_200_0, $line;
    push @grep_filenames_200_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_200_0 = grep { /pattern/msx } @grep_lines_200_0;
    $grep_result_200_0 = @grep_filtered_200_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_200_0 > 0 ? 0 : 1;
    $output_200 = $grep_result_200_0;
    $output_200 = $grep_result_200_0;

        my $set1_201 = "\\0";
    my $set2_201 = "\\n";
    my $input_201 = $output_200;
    # Expand character ranges for tr command
    my $expanded_set1_201 = $set1_201;
    my $expanded_set2_201 = $set2_201;
    # Handle a-z range in set1
    if ($expanded_set1_201 =~ /a-z/msx) {
    $expanded_set1_201 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_201 =~ /A-Z/msx) {
    $expanded_set1_201 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_201 =~ /a-z/msx) {
    $expanded_set2_201 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_201 =~ /A-Z/msx) {
    $expanded_set2_201 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_200_1 = q{};
    for my $char ( split //msx, $input_201 ) {
    my $pos_201 = index $expanded_set1_201, $char;
    if ( $pos_201 >= 0 && $pos_201 < length $expanded_set2_201 ) {
    $tr_result_200_1 .= substr $expanded_set2_201, $pos_201, 1;
    } else {
    $tr_result_200_1 .= $char;
    }
    }
    if (!($tr_result_200_1 =~ m{\n\z}msx || $tr_result_200_1 eq q{})) {
    $tr_result_200_1 .= "\n";
    }
    $output_200 = $tr_result_200_1;
    $output_200 = $tr_result_200_1;
    if ($output_200 ne q{} && !defined $output_printed_200) {
        print $output_200;
        if (!($output_200 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_200 ) { $main_exit_code = 1; }
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

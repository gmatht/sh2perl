#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
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
    my $output_202;
    my $output_printed_202;
    my $pipeline_success_202 = 1;
    $output_202 .= "text with pattern\n";
if ( !($output_202 =~ m{\n\z}msx) ) { $output_202 .= "\n"; }

        my $grep_result_202_1;
    my @grep_lines_202_1 = split /\n/msx, $output_202;
    my @grep_filtered_202_1 = grep { /PATTERN/msxi } @grep_lines_202_1;
    $grep_result_202_1 = join "\n", @grep_filtered_202_1;
    if (!($grep_result_202_1 =~ m{\n\z}msx || $grep_result_202_1 eq q{})) {
    $grep_result_202_1 .= "\n";
    }
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
    }
# Original bash: echo -e "line1\nline2\nline3" | grep -v "line2"
{
    my $output_203;
    my $output_printed_203;
    my $pipeline_success_203 = 1;
    $output_203 .= "line1\nline2\nline3";
if ( !($output_203 =~ m{\n\z}msx) ) { $output_203 .= "\n"; }

        my $grep_result_203_1;
    my @grep_lines_203_1 = split /\n/msx, $output_203;
    my @grep_filtered_203_1 = grep { !/line2/msx } @grep_lines_203_1;
    $grep_result_203_1 = join "\n", @grep_filtered_203_1;
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
    }
# Original bash: echo -e "match\nno match\nmatch again" | grep -c "match"
{
    my $output_204;
    my $output_printed_204;
    my $pipeline_success_204 = 1;
    $output_204 .= "match\nno match\nmatch again";
if ( !($output_204 =~ m{\n\z}msx) ) { $output_204 .= "\n"; }

        my $grep_result_204_1;
    my @grep_lines_204_1 = split /\n/msx, $output_204;
    my @grep_filtered_204_1 = grep { /match/msx } @grep_lines_204_1;
    $grep_result_204_1 = scalar @grep_filtered_204_1;
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
    }
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET"
{
    my $output_205;
    my $output_printed_205;
    my $pipeline_success_205 = 1;
    $output_205 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_205 =~ m{\n\z}msx) ) { $output_205 .= "\n"; }

        my $grep_result_205_1;
    my @grep_lines_205_1 = split /\n/msx, $output_205;
    my @grep_filtered_205_1 = grep { /TARGET/msx } @grep_lines_205_1;
    my @grep_with_context_205_1;
    for my $i (0..@grep_lines_205_1-1) {
    if (scalar grep { $_ eq $grep_lines_205_1[$i] } @grep_filtered_205_1) {
    push @grep_with_context_205_1, $grep_lines_205_1[$i];
    for my $j (($i + 1)..($i + 2)) {
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
    }
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET"
{
    my $output_206;
    my $output_printed_206;
    my $pipeline_success_206 = 1;
    $output_206 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_206 =~ m{\n\z}msx) ) { $output_206 .= "\n"; }

        my $grep_result_206_1;
    my @grep_lines_206_1 = split /\n/msx, $output_206;
    my @grep_filtered_206_1 = grep { /TARGET/msx } @grep_lines_206_1;
    my @grep_with_context_206_1;
    for my $i (0..@grep_lines_206_1-1) {
    if (scalar grep { $_ eq $grep_lines_206_1[$i] } @grep_filtered_206_1) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_206_1, $grep_lines_206_1[$j];
    }
    }
    push @grep_with_context_206_1, $grep_lines_206_1[$i];
    }
    }
    $grep_result_206_1 = join "\n", @grep_with_context_206_1;
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
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET"
{
    my $output_207;
    my $output_printed_207;
    my $pipeline_success_207 = 1;
    $output_207 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_207 =~ m{\n\z}msx) ) { $output_207 .= "\n"; }

        my $grep_result_207_1;
    my @grep_lines_207_1 = split /\n/msx, $output_207;
    my @grep_filtered_207_1 = grep { /TARGET/msx } @grep_lines_207_1;
    my @grep_with_context_207_1;
    for my $i (0..@grep_lines_207_1-1) {
    if (scalar grep { $_ eq $grep_lines_207_1[$i] } @grep_filtered_207_1) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_207_1, $grep_lines_207_1[$j];
    }
    }
    push @grep_with_context_207_1, $grep_lines_207_1[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_207_1, $grep_lines_207_1[$j];
    }
    }
    }
    $grep_result_207_1 = join "\n", @grep_with_context_207_1;
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
print "== File handling parameters ==\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'temp_file.txt'
      or die "Cannot open file: $!\n";
    print "content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
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
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_208 = grep { /content/msx } @grep_lines_208;
my @grep_with_filename_208;
for my $line (@grep_filtered_208) {
    push @grep_with_filename_208, "temp_file.txt:$line";
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
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_209 = grep { /content/msx } @grep_lines_209;
$grep_result_209 = join "\n", @grep_filtered_209;
if (!($grep_result_209 =~ m{\n\z}msx || $grep_result_209 eq q{})) {
    $grep_result_209 .= "\n";
}
print $grep_result_209;
$CHILD_ERROR = scalar @grep_filtered_209 > 0 ? 0 : 1;
my $grep_result_210;
my @grep_lines_210 = ();
my @grep_filenames_210 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_210, $line;
        push @grep_filenames_210, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_210 = grep { /content/msx } @grep_lines_210;
$grep_result_210 = @grep_filtered_210 > 0 ? "temp_file.txt" : "";
print $grep_result_210;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_210 > 0 ? 0 : 1;
my $grep_result_211;
my @grep_lines_211 = ();
my @grep_filenames_211 = ();
if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_211, $line;
        push @grep_filenames_211, "temp_file.txt";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
my @grep_filtered_211 = grep { /nonexistent/msx } @grep_lines_211;
$grep_result_211 = @grep_filtered_211 == 0 ? "temp_file.txt" : "";
print $grep_result_211;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_211 > 0 ? 0 : 1;
if ($CHILD_ERROR != 0) {
    1;
}
print "== Output formatting parameters ==\n";
# Original bash: echo "text with pattern in it" | grep -o "pattern"
{
    my $output_213;
    my $output_printed_213;
    my $pipeline_success_213 = 1;
    $output_213 .= "text with pattern in it\n";
if ( !($output_213 =~ m{\n\z}msx) ) { $output_213 .= "\n"; }

        my $grep_result_213_1;
    my @grep_lines_213_1 = split /\n/msx, $output_213;
    my @grep_filtered_213_1 = grep { /pattern/msx } @grep_lines_213_1;
    my @grep_matches_213_1;
    foreach my $line (@grep_filtered_213_1) {
    if ($line =~ /(pattern)/msx) {
    push @grep_matches_213_1, $1;
    }
    }
    $grep_result_213_1 = join "\n", @grep_matches_213_1;
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
    }
# Original bash: echo "text with pattern in it" | grep -b "pattern"
{
    my $output_214;
    my $output_printed_214;
    my $pipeline_success_214 = 1;
    $output_214 .= "text with pattern in it\n";
if ( !($output_214 =~ m{\n\z}msx) ) { $output_214 .= "\n"; }

        my $grep_result_214_1;
    my @grep_lines_214_1 = split /\n/msx, $output_214;
    my @grep_filtered_214_1 = grep { /pattern/msx } @grep_lines_214_1;
    my @grep_with_offset_214_1;
    my $offset_214_1 = 0;
    for my $line (@grep_lines_214_1) {
    if (grep { $_ eq $line } @grep_filtered_214_1) {
    push @grep_with_offset_214_1, sprintf "%d:%s", $offset_214_1, $line;
    }
    $offset_214_1 += length($line) + 1; # +1 for newline
    }
    $grep_result_214_1 = join "\n", @grep_with_offset_214_1;
    if (!($grep_result_214_1 =~ m{\n\z}msx || $grep_result_214_1 eq q{})) {
    $grep_result_214_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_214_1 > 0 ? 0 : 1;
    $output_214 = $grep_result_214_1;
    $output_214 = $grep_result_214_1;
    if ((scalar @grep_filtered_214_1) == 0) {
        $pipeline_success_214 = 0;
    }
    if ($output_214 ne q{} && !defined $output_printed_214) {
        print $output_214;
        if (!($output_214 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_214 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -n "pattern"
{
    my $output_215;
    my $output_printed_215;
    my $pipeline_success_215 = 1;
    $output_215 .= "text with pattern in it\n";
if ( !($output_215 =~ m{\n\z}msx) ) { $output_215 .= "\n"; }

        my $grep_result_215_1;
    my @grep_lines_215_1 = split /\n/msx, $output_215;
    my @grep_filtered_215_1 = grep { /pattern/msx } @grep_lines_215_1;
    my @grep_numbered_215_1;
    for my $i (0..@grep_lines_215_1-1) {
    if (scalar grep { $_ eq $grep_lines_215_1[$i] } @grep_filtered_215_1) {
    push @grep_numbered_215_1, sprintf "%d:%s", $i + 1, $grep_lines_215_1[$i];
    }
    }
    $grep_result_215_1 = join "\n", @grep_numbered_215_1;
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
print "== Recursive and include/exclude parameters ==\n";
use File::Path qw(make_path);
my $err;
if ( !-d 'test_dir' ) {
    make_path( 'test_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory 'test_dir': $err->[0]\n";
    }
}
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'test_dir/file1.txt'
      or die "Cannot open file: $!\n";
    print "pattern here\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'test_dir/file2.txt'
      or die "Cannot open file: $!\n";
    print "no pattern\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
my $grep_result_217;
my @grep_lines_217 = ();
my @grep_filenames_217 = ();
sub find_files_recursive_217 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_217($path, $pattern));
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
my @files_217 = find_files_recursive_217('test_dir', '*');
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
        push @grep_with_filename_217, "$grep_filenames_217[$i]:$grep_lines_217[$i]";
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
sub find_files_recursive_218 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_218($path, $pattern));
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
my @files_218 = find_files_recursive_218('test_dir', '*.txt');
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
my @grep_with_filename_218;
for my $i (0..@grep_lines_218-1) {
    if (scalar grep { $_ eq $grep_lines_218[$i] } @grep_filtered_218) {
        push @grep_with_filename_218, "$grep_filenames_218[$i]:$grep_lines_218[$i]";
    }
}
$grep_result_218 = join "\n", @grep_with_filename_218;
if (!($grep_result_218 =~ m{\n\z}msx || $grep_result_218 eq q{})) {
    $grep_result_218 .= "\n";
}
print $grep_result_218;
$CHILD_ERROR = scalar @grep_filtered_218 > 0 ? 0 : 1;
my $grep_result_219;
my @grep_lines_219 = ();
my @grep_filenames_219 = ();
sub find_files_recursive_219 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_219($path, $pattern));
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
my @files_219 = find_files_recursive_219('test_dir', '*');
for my $file (@files_219) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_219, $line;
            push @grep_filenames_219, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_219 = grep { /pattern/msx } @grep_lines_219;
my @grep_with_filename_219;
for my $i (0..@grep_lines_219-1) {
    if (scalar grep { $_ eq $grep_lines_219[$i] } @grep_filtered_219) {
        push @grep_with_filename_219, "$grep_filenames_219[$i]:$grep_lines_219[$i]";
    }
}
$grep_result_219 = join "\n", @grep_with_filename_219;
if (!($grep_result_219 =~ m{\n\z}msx || $grep_result_219 eq q{})) {
    $grep_result_219 .= "\n";
}
print $grep_result_219;
$CHILD_ERROR = scalar @grep_filtered_219 > 0 ? 0 : 1;
my $grep_result_220;
my @grep_lines_220 = ();
my @grep_filenames_220 = ();
sub find_files_recursive_220 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
        while (my $file = readdir $dh) {
            next if $file eq '.' || $file eq '..';
            my $path = "$dir/$file";
            if (-d $path) {
                @files = (@files, find_files_recursive_220($path, $pattern));
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
my @files_220 = find_files_recursive_220('test_dir', '*.txt');
for my $file (@files_220) {
    if (-f $file) {
        open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_220, $line;
            push @grep_filenames_220, $file;
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
}
my @grep_filtered_220 = grep { /pattern/msx } @grep_lines_220;
my %file_counts_220;
for my $i (0..@grep_lines_220-1) {
    if (scalar grep { $_ eq $grep_lines_220[$i] } @grep_filtered_220) {
        $file_counts_220{$grep_filenames_220[$i]}++;
    }
}
$grep_result_220 = q{};
for my $file (sort keys %file_counts_220) {
    $grep_result_220 .= "$file:$file_counts_220{$file}\n";
}
$grep_result_220 =~ s/\\n$/msx; # Remove trailing newline
print $grep_result_220;
print "\n";
$CHILD_ERROR = scalar @grep_filtered_220 > 0 ? 0 : 1;
# Original bash: grep -r "pattern" test_dir --include="*.txt" | wc -l
{
    my $output_221;
    my $output_printed_221;
    my $pipeline_success_221 = 1;
        my $grep_result_221_0;
    my @grep_lines_221_0 = ();
    my @grep_filenames_221_0 = ();
    sub find_files_recursive_221_0 {
    my ($dir, $pattern) = @_;
    my @files;
    if ( opendir my $dh, $dir ) {
    while (my $file = readdir $dh) {
    next if $file eq '.' || $file eq '..';
    my $path = "$dir/$file";
    if (-d $path) {
    @files = (@files, find_files_recursive_221_0($path, $pattern));
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
    my @files_221_0 = find_files_recursive_221_0('test_dir', '*.txt');
    for my $file (@files_221_0) {
    if (-f $file) {
    open my $fh, '<', $file or die "Cannot open $file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_221_0, $line;
    push @grep_filenames_221_0, $file;
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    }
    my @grep_filtered_221_0 = grep { /pattern/msx } @grep_lines_221_0;
    my @grep_with_filename_221_0;
    for my $i (0..@grep_lines_221_0-1) {
    if (scalar grep { $_ eq $grep_lines_221_0[$i] } @grep_filtered_221_0) {
    push @grep_with_filename_221_0, "$grep_filenames_221_0[$i]:$grep_lines_221_0[$i]";
    }
    }
    $grep_result_221_0 = join "\n", @grep_with_filename_221_0;
    if (!($grep_result_221_0 =~ m{\n\z}msx || $grep_result_221_0 eq q{})) {
    $grep_result_221_0 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_221_0 > 0 ? 0 : 1;
    $output_221 = $grep_result_221_0;
    $output_221 = $grep_result_221_0;
    if ((scalar @grep_filtered_221_0) == 0) {
        $pipeline_success_221 = 0;
    }

        use IPC::Open3;
    my @wc_args_221_1 = ("-l");
    my ($wc_in_221_1, $wc_out_221_1, $wc_err_221_1);
    my $wc_pid_221_1 = open3($wc_in_221_1, $wc_out_221_1, $wc_err_221_1, 'wc', @wc_args_221_1);
    print {$wc_in_221_1} $output_221;
    close $wc_in_221_1 or die "Close failed: $!\n";
    my $output_221_1 = do { local $/ = undef; <$wc_out_221_1> };
    close $wc_out_221_1 or die "Close failed: $!\n";
    waitpid $wc_pid_221_1, 0;
    $output_221 = $output_221_1;
    if ($output_221 ne q{} && !defined $output_printed_221) {
        print $output_221;
        if (!($output_221 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_221 ) { $main_exit_code = 1; }
    }
print "== Advanced parameters ==\n";
# Original bash: echo -e "match1\nmatch2\nmatch3\nmatch4" | grep -m 2 "match"
{
    my $output_222;
    my $output_printed_222;
    my $pipeline_success_222 = 1;
    $output_222 .= "match1\nmatch2\nmatch3\nmatch4";
if ( !($output_222 =~ m{\n\z}msx) ) { $output_222 .= "\n"; }

        my $grep_result_222_1;
    my @grep_lines_222_1 = split /\n/msx, $output_222;
    my @grep_filtered_222_1 = grep { /match/msx } @grep_lines_222_1;
    @grep_filtered_222_1 = @grep_filtered_222_1[0..1];
    $grep_result_222_1 = join "\n", @grep_filtered_222_1;
    if (!($grep_result_222_1 =~ m{\n\z}msx || $grep_result_222_1 eq q{})) {
    $grep_result_222_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_222_1 > 0 ? 0 : 1;
    $output_222 = $grep_result_222_1;
    $output_222 = $grep_result_222_1;
    if ((scalar @grep_filtered_222_1) == 0) {
        $pipeline_success_222 = 0;
    }
    if ($output_222 ne q{} && !defined $output_printed_222) {
        print $output_222;
        if (!($output_222 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_222 ) { $main_exit_code = 1; }
    }
# Original bash: echo "text with pattern in it" | grep -q "pattern" && echo "found" || echo "not found"
{
    my $output_223;
    my $output_printed_223;
    my $pipeline_success_223 = 1;
    $output_223 .= "text with pattern in it\n";
if ( !($output_223 =~ m{\n\z}msx) ) { $output_223 .= "\n"; }

        my $grep_exit_code_224;
    {
    my $grep_result_223_1;
    my @grep_lines_223_1 = split /\n/msx, $output_223;
    my @grep_filtered_223_1 = grep { /pattern/msx } @grep_lines_223_1;
    $grep_result_223_1 = join "\n", @grep_filtered_223_1;
    if (!($grep_result_223_1 =~ m{\n\z}msx || $grep_result_223_1 eq q{})) {
    $grep_result_223_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_223_1 > 0 ? 0 : 1;
    $output_223 = $grep_result_223_1;
    $grep_exit_code_224 = scalar @grep_filtered_223_1 > 0 ? 0 : 1;
    if ($grep_exit_code_224 == 0) {
    print "found\n";
    } else {
    print "not found\n";
    }
    }
    $pipeline_success_223 = 1;
    $output_223 = q{};
    if ($output_223 ne q{} && !defined $output_printed_223) {
        print $output_223;
        if (!($output_223 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_223 ) { $main_exit_code = 1; }
    }
# Original bash: grep -Z -l "pattern" temp_file.txt | tr '\0' '\n'
{
    my $output_225;
    my $output_printed_225;
    my $pipeline_success_225 = 1;
        my $grep_result_225_0;
    my @grep_lines_225_0 = ();
    my @grep_filenames_225_0 = ();
    if (-e "temp_file.txt") {
    open my $fh, '<', "temp_file.txt" or croak "Cannot open file: $ERRNO";
    while (my $line = <$fh>) {
    chomp $line;
    push @grep_lines_225_0, $line;
    push @grep_filenames_225_0, "temp_file.txt";
    }
    close $fh
    or croak "Close failed: $OS_ERROR";
    }
    else { print STDERR "grep: temp_file.txt: No such file or directory\n"; }
    my @grep_filtered_225_0 = grep { /pattern/msx } @grep_lines_225_0;
    $grep_result_225_0 = @grep_filtered_225_0 > 0 ? "temp_file.txt" : "";
    $CHILD_ERROR = scalar @grep_filtered_225_0 > 0 ? 0 : 1;
    $output_225 = $grep_result_225_0;
    $output_225 = $grep_result_225_0;
    if ((scalar @grep_filtered_225_0) == 0) {
        $pipeline_success_225 = 0;
    }

        my $set1_226 = "\\0";
    my $set2_226 = "\\n";
    my $input_226 = $output_225;
    # Expand character ranges for tr command
    my $expanded_set1_226 = $set1_226;
    my $expanded_set2_226 = $set2_226;
    # Handle a-z range in set1
    if ($expanded_set1_226 =~ /a-z/msx) {
    $expanded_set1_226 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set1
    if ($expanded_set1_226 =~ /A-Z/msx) {
    $expanded_set1_226 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    # Handle a-z range in set2
    if ($expanded_set2_226 =~ /a-z/msx) {
    $expanded_set2_226 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
    }
    # Handle A-Z range in set2
    if ($expanded_set2_226 =~ /A-Z/msx) {
    $expanded_set2_226 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
    }
    my $tr_result_225_1 = q{};
    for my $char ( split //msx, $input_226 ) {
    my $pos_226 = index $expanded_set1_226, $char;
    if ( $pos_226 >= 0 && $pos_226 < length $expanded_set2_226 ) {
    $tr_result_225_1 .= substr $expanded_set2_226, $pos_226, 1;
    } else {
    $tr_result_225_1 .= $char;
    }
    }
    if (!($tr_result_225_1 =~ m{\n\z}msx || $tr_result_225_1 eq q{})) {
    $tr_result_225_1 .= "\n";
    }
    $output_225 = $tr_result_225_1;
    if ($output_225 ne q{} && !defined $output_printed_225) {
        print $output_225;
        if (!($output_225 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_225 ) { $main_exit_code = 1; }
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
    carp "rm: carping: ", "temp_file.txt", ": No such file or directory\n";
}
if ( -e "test_dir" ) {
    if ( -d "test_dir" ) {
        my $err;
        remove_tree("test_dir", {error => \$err});
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
    carp "rm: carping: ", "test_dir", ": No such file or directory\n";
}

exit $main_exit_code;

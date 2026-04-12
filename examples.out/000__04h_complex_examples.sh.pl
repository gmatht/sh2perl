#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
sub capture_stdout {
    my ($code) = @_;
    my $captured = q{};
    {
        local *STDOUT;
        open STDOUT, '>', \$captured
          or die "Cannot capture stdout: $!\n";
        $code->();
    }
    return $captured;
}


my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "=== Complex Backtick Examples ===\n";
my $nested_result = ("Three wells: " . (do { my $_chomp_temp = do {
    my $_chomp_result = do { my $head_line_count = 0;
my $output_0 = q{};
while (1) {
    my $line = 'well';
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_0 .= "\n"; }
    $output_0 .= $line;
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0 };
    chomp $_chomp_result;
    $_chomp_result;
}; chomp $_chomp_temp; $_chomp_temp; }));
do {
    my $output = "Nested backticks: $nested_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $count = do {
    my $output_116;
    my $pipeline_success_116 = 1;
    $output_116 = do {
        my @ls_files_117 = ();
        if ( -f q{.} ) {
            push @ls_files_117, q{.};
        }
        elsif ( -d q{.} ) {
            if ( opendir my $dh, q{.} ) {
                while ( my $file = readdir $dh ) {
                    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                    push @ls_files_117, $file;
                }
                closedir $dh;
                @ls_files_117 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_117;
            }
        }
        (@ls_files_117 ? join("\n", @ls_files_117) . "\n" : q{});
    };
    use IPC::Open3;
    my @wc_args_116_1 = ("-l");
    my ($wc_in_116_1, $wc_out_116_1, $wc_err_116_1);
    my $wc_pid_116_1 = open3($wc_in_116_1, $wc_out_116_1, $wc_err_116_1, 'wc', @wc_args_116_1);
    print {$wc_in_116_1} $output_116;
    close $wc_in_116_1 or die "Close failed: $!\n";
    $output_116 = do { local $/ = undef; <$wc_out_116_1> };
    close $wc_out_116_1 or die "Close failed: $!\n";
    waitpid $wc_pid_116_1, 0;
    if ( !$pipeline_success_116 ) { $main_exit_code = 1; }
    $output_116 =~ s/\n+\z//msx;
    $output_116;
};
do {
    my $output = "File count: $count";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $current_user;
$current_user = ('root');
if ("$current_user" eq "root") {
    print "Running as root\n";
}
else {
    print "Not running as root\n";
}
my $system_name;
$system_name = "Darwin";
if ($system_name =~ /^Linux$/msx) {
        print "Running on Linux\n";
} elsif ($system_name =~ /^Darwin$/msx) {
        print "Running on macOS\n";
} elsif ($system_name =~ /^.*$/msx) {
        print "Running on other " . "sys" . "tem\n";
}

sub get_file_size {
    my ($file) = @_;
    my $size = do {
my $wc_input_119 = do {
    local $INPUT_RECORD_SEPARATOR = undef;
    open my $fh, '<', "$file"
        or croak "Cannot open file: $OS_ERROR";
    my $content = <$fh>;
    close $fh
        or croak "Close failed: $OS_ERROR";
    $content
};
use IPC::Open3;
my @wc_args_119 = ("-c");
my ($wc_in_119, $wc_out_119, $wc_err_119);
my $wc_pid_119 = open3($wc_in_119, $wc_out_119, $wc_err_119, 'wc', @wc_args_119);
print {$wc_in_119} $wc_input_119;
close $wc_in_119 or die "Close failed: $!\n";
my $wc_output_119 = do { local $/ = undef; <$wc_out_119> };
close $wc_out_119 or die "Close failed: $!\n";
waitpid $wc_pid_119, 0;
    chomp $wc_output_119;
    $wc_output_119;
};
    do {
    my $output = "File $file has $size bytes";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
    return;
}
get_file_size('000__01_file_directory_operations.sh');
my @files = ((grep { !/\//msx } glob '*.sh'), (glob 'examples/*.sh'));
print "Shell scripts found: " . scalar(@files) . "\n";
my $file;
for my $file (@files) {
    do {
    my $output = "  - $file";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
}
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'file1.txt'
      or die "Cannot open file: $!\n";
    print "apple
banana
cherry\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'file2.txt'
      or die "Cannot open file: $!\n";
    print "banana
cherry
date\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
};
my $process_result = do { my $temp_file_ps_120 = q{/tmp} . '/process_sub_120.tmp';
{
    open my $fh, '>', $temp_file_ps_120 or croak "Cannot create temp file: $OS_ERROR\n";
    my $temp_output = q{};
    $temp_output .= my $file_content_121 = do {
    local $INPUT_RECORD_SEPARATOR = undef;
    open my $fh, '<', 'file1.txt'
        or croak "Cannot open file: $OS_ERROR";
    my $content = <$fh>;
    close $fh
        or croak "Close failed: $OS_ERROR";
    $content
};
my @sort_lines_121 = split /\n/msx, $file_content_121;
my @sort_sorted_121 = sort @sort_lines_121;
my $sort_output_121 = join "\n", @sort_sorted_121;
if ($sort_output_121 ne q{} && !($sort_output_121 =~ m{\n\z}msx)) {
    $sort_output_121 .= "\n";
}
$file_content_121 = $sort_output_121;
;
    print {$fh} $temp_output;
    close $fh
        or croak "Close failed: $OS_ERROR\n";
}
my $temp_file_ps_122 = q{/tmp} . '/process_sub_122.tmp';
{
    open my $fh, '>', $temp_file_ps_122 or croak "Cannot create temp file: $OS_ERROR\n";
    my $temp_output = q{};
    $temp_output .= my $file_content_123 = do {
    local $INPUT_RECORD_SEPARATOR = undef;
    open my $fh, '<', 'file2.txt'
        or croak "Cannot open file: $OS_ERROR";
    my $content = <$fh>;
    close $fh
        or croak "Close failed: $OS_ERROR";
    $content
};
my @sort_lines_123 = split /\n/msx, $file_content_123;
my @sort_sorted_123 = sort @sort_lines_123;
my $sort_output_123 = join "\n", @sort_sorted_123;
if ($sort_output_123 ne q{} && !($sort_output_123 =~ m{\n\z}msx)) {
    $sort_output_123 .= "\n";
}
$file_content_123 = $sort_output_123;
;
    print {$fh} $temp_output;
    close $fh
        or croak "Close failed: $OS_ERROR\n";
}
 my @file1_lines;
my @file2_lines;
if (open my $fh1, '<', $temp_file_ps_120) {
    while (my $line = <$fh1>) {
        chomp $line;
        push @file1_lines, $line;
    }
    close $fh1 or croak "Close failed: $OS_ERROR";
}
if (open my $fh2, '<', $temp_file_ps_122) {
    while (my $line = <$fh2>) {
        chomp $line;
        push @file2_lines, $line;
    }
    close $fh2 or croak "Close failed: $OS_ERROR";
}
my %file1_set = map { $_ => 1 } @file1_lines;
my %file2_set = map { $_ => 1 } @file2_lines;
my @common_lines;
foreach my $line (@file1_lines) {
    if (exists $file2_set{$line}) {
        push @common_lines, $line;
    }
}
my $comm_output = q{};
foreach my $line (@file1_lines) {
    if (!exists $file2_set{$line}) {
        $comm_output .= $line . "\n";
    }
}
$comm_output =~ s/\n$//msx;
$comm_output };
print "Process substitution result:\n";
print $process_result;
if ( !( $process_result =~ m{\n\z}msx ) ) { print "\n"; }
my $here_string_result = do { my $input_data = "hello world"; my $set1_125 = 'a-z';
my $set2_125 = 'A-Z';
my $input_125 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_125 = $set1_125;
my $expanded_set2_125 = $set2_125;
# Handle a-z range in set1
if ($expanded_set1_125 =~ /a-z/msx) {
    $expanded_set1_125 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_125 =~ /A-Z/msx) {
    $expanded_set1_125 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle a-z range in set2
if ($expanded_set2_125 =~ /a-z/msx) {
    $expanded_set2_125 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_125 =~ /A-Z/msx) {
    $expanded_set2_125 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
my $tr_result_124 = q{};
for my $char ( split //msx, $input_125 ) {
    my $pos_125 = index $expanded_set1_125, $char;
    if ( $pos_125 >= 0 && $pos_125 < length $expanded_set2_125 ) {
        $tr_result_124 .= substr $expanded_set2_125, $pos_125, 1;
    } else {
        $tr_result_124 .= $char;
    }
}
$tr_result_124 };
do {
    my $output = "Here string result: $here_string_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $perl_result = do {
    my $result;
    my $eval_success = eval {
        $result = capture_stdout( sub { print "Hello from Perl\n" } );
        1;
    };
    if ( !$eval_success ) {
        $result = "Error executing Perl code: $EVAL_ERROR";
    }
    $result;
};
do {
    my $output = "Perl result: $perl_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
if ( -e "file1.txt" ) {
    if ( -d "file1.txt" ) {
        carp "rm: carping: ", "file1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "file1.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "file1.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
    carp "rm: carping: ", "file1.txt", ": No such file or directory\n";
}
if ( -e "file2.txt" ) {
    if ( -d "file2.txt" ) {
        carp "rm: carping: ", "file2.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "file2.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "file2.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
    carp "rm: carping: ", "file2.txt", ": No such file or directory\n";
}
print "=== Complex Backtick Examples Complete ===\n";

exit $main_exit_code;

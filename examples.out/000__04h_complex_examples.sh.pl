#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
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
my $nested_result = ("Three wells: " . (do { my $_chomp_temp = do { my $_pipeline_result = do {
    do { my $output_113 = q{};
my $output_printed_113;
my $head_line_count = 0;
while (1) {
    my $line = 'well';
    if ($head_line_count < 3) {
    $output_113 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_113; };
}; $_pipeline_result =~ s/\n+\z//msx; $_pipeline_result; }; chomp $_chomp_temp; $_chomp_temp; }));
do {
    my $output = "Nested backticks: $nested_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $count = do { my $_pipeline_result = do {
    my $output_114 = q{};
    my $output_printed_114;
    my $pipeline_success_114 = 1;
    $output_114 = do {
        my @ls_files_115 = ();
        if ( -f q{.} ) {
            push @ls_files_115, q{.};
        }
        elsif ( -d q{.} ) {
            if ( opendir my $dh, q{.} ) {
                while ( my $file = readdir $dh ) {
                    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                    push @ls_files_115, $file;
                }
                closedir $dh;
                @ls_files_115 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_115;
            }
        }
        (@ls_files_115 ? join("\n", @ls_files_115) . "\n" : q{});
    };
    ;
    if ($CHILD_ERROR != 0) { $pipeline_success_114 = 0; }
    use IPC::Open3;
    my @wc_args_114_1 = ('-l');
    my ($wc_in_114_1, $wc_out_114_1, $wc_err_114_1);
    my $wc_pid_114_1 = open3($wc_in_114_1, $wc_out_114_1, $wc_err_114_1, 'wc', @wc_args_114_1);
    print {$wc_in_114_1} $output_114;
    close $wc_in_114_1 or die "Close failed: $!\n";
    $output_114 = do { local $/ = undef; <$wc_out_114_1> };
    if ($output_114 eq q{}) { $output_114 = "0\n"; }
    close $wc_out_114_1 or die "Close failed: $!\n";
    waitpid $wc_pid_114_1, 0;
    if ( !$pipeline_success_114 ) { $main_exit_code = 1; }
    $output_114;
}; $_pipeline_result =~ s/\n+\z//msx; $_pipeline_result; };
do {
    my $output = "File count: $count";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $current_user;
$current_user = ('root');
if ("$current_user" eq "root") {
    print "Running as root\n";
}
else {
    print "Not running as root\n";
}
my $system_name;
$system_name = 'Darwin';
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
    local $ENV{file} = $file;
    my ($in_117, $out_117, $err_117);
    my $pid = open3($in_117, $out_117, $err_117, 'bash', '-c', 'wc -c < "$file"');
    close $in_117 or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_117> };
    close $out_117 or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result =~ s/\n+\z//msx;
    $result;
};
    do {
    my $output = "File $file has $size bytes";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
    $CHILD_ERROR = 0;
    return;
}
get_file_size('000__01_file_directory_operations.sh');
my @files = ((grep { !/\//msx } glob '*.sh'), (glob 'examples/*.sh'));
print "Shell scripts found: " . scalar(@files) . "\n";
$CHILD_ERROR = 0;
my $file;
for my $file (@files) {
    do {
    my $output = "  - $file";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
    $CHILD_ERROR = 0;
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
my $process_result = do {
    my ($in_118, $out_118, $err_118);
    my $pid = open3($in_118, $out_118, $err_118, 'bash', '-c', 'comm -23 <(sort file1.txt) <(sort file2.txt)');
    close $in_118 or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_118> };
    close $out_118 or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result =~ s/\n+\z//msx;
    $result;
};
print "Process substitution result:\n";
print $process_result;
if ( !( $process_result =~ m{\n\z}msx ) ) { print "\n"; }
my $here_string_result = do {
    my ($in_119, $out_119, $err_119);
    my $pid = open3($in_119, $out_119, $err_119, 'bash', '-c', q{tr a-z A-Z <<< 'hello world'});
    close $in_119 or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_119> };
    close $out_119 or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result =~ s/\n+\z//msx;
    $result;
};
do {
    my $output = "Here string result: $here_string_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
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
$CHILD_ERROR = 0;
if ( -e "file1.txt" ) {
    if ( -d "file1.txt" ) {
        carp "rm: carping: ", "file1.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "file1.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "file1.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "file2.txt" ) {
    if ( -d "file2.txt" ) {
        carp "rm: carping: ", "file2.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "file2.txt" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "file2.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
print "=== Complex Backtick Examples Complete ===\n";

exit $main_exit_code;

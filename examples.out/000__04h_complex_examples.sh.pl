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
          or die "Cannot capture stdout: $OS_ERROR\n";
        $code->();
    }
    return $captured;
}


my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

print "=== Complex Backtick Examples ===\n";
my $nested_result = ("Three wells: " . (do { my $_chomp_temp = do { do {
    do { my $output_112 = q{};
my $output_printed_112;
my $head_line_count = 0;
while (1) {
    my $line = 'well';
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    $output_112 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_112 };
} }; chomp $_chomp_temp; $_chomp_temp; }));
do {
    my $output = "Nested backticks: $nested_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $count = do { do {
    my $output_113 = q{};
    my $output_printed_113;
    my $pipeline_success_113 = 1;
    $output_113 = do {
        my @ls_files_114 = ();
        if ( -f q{.} ) {
            push @ls_files_114, q{.};
        }
        elsif ( -d q{.} ) {
            if ( opendir my $dh, q{.} ) {
                while ( my $file = readdir $dh ) {
                    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                    push @ls_files_114, $file;
                }
                closedir $dh;
                @ls_files_114 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_114;
            }
        }
        (@ls_files_114 ? join("\n", @ls_files_114) . "\n" : q{});
    };
    use IPC::Open3;
    my @wc_args_113_1 = ('-l');
    my ($wc_in_113_1, $wc_out_113_1, $wc_err_113_1);
    my $wc_pid_113_1 = open3($wc_in_113_1, $wc_out_113_1, $wc_err_113_1, 'wc', @wc_args_113_1);
    print {$wc_in_113_1} $output_113;
    close $wc_in_113_1 or die "Close failed: $OS_ERROR\n";
    $output_113 = do { local $/ = undef; <$wc_out_113_1> };
    if ($output_113 eq q{}) { $output_113 = "0\n"; }
    close $wc_out_113_1 or die "Close failed: $OS_ERROR\n";
    waitpid $wc_pid_113_1, 0;
    if ( !$pipeline_success_113 ) { $main_exit_code = 1; }
    $output_113 =~ s/\n+\z//msx;
    $output_113;
} };
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
    my $size = do { my $command = "wc -c < \"$file\""; chomp(my $result = qx{$command}); $CHILD_ERROR = $? >> 8; $result; };
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
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file1.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "apple
banana
cherry\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'file2.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "banana
cherry
date\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $process_result = do { my $command = "bash -c 'comm -23 <(sort file1.txt) <(sort file2.txt)'"; chomp(my $result = qx{$command}); $CHILD_ERROR = $? >> 8; $result; };
print "Process substitution result:\n";
print $process_result;
if ( !( ($process_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $here_string_result = do { my $here_input = "hello world"; chomp(my $result = qx{echo "$here_input" | tr a-z A-Z}); $CHILD_ERROR = $? >> 8; $result; };
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
}
print "=== Complex Backtick Examples Complete ===\n";

exit $main_exit_code;

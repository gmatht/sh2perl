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

$PROGRAM_NAME = '000__04h_complex_examples.sh';
my $current_user;
my @current_user;
my %current_user;

print "=== Complex Backtick Examples ===\n";
my $nested_result;
my @nested_result;
my %nested_result;
$nested_result = ("Three wells: " . (do { my $_chomp_temp = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    do { my $output_111 = q{};
my $output_printed_111;
my $head_line_count = 0;
while (1) {
    my $line = 'well';
    if ($head_line_count < 3) {
    $output_111 .= $line . "\n";
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_111 };
}; $_pipeline_result; }; chomp $_chomp_temp; $_chomp_temp; }));
do {
    my $output = "Nested backticks: $nested_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $count;
my @count;
my %count;
$count = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $output_112 = q{};
    my $output_printed_112;
    my $pipeline_success_112 = 1;
    $output_112 = do {
        my @ls_files_113 = ();
        if ( -f q{.} ) {
            push @ls_files_113, q{.};
        }
        elsif ( -d q{.} ) {
            if ( opendir my $dh, q{.} ) {
                while ( my $file = readdir $dh ) {
                    next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                    push @ls_files_113, $file;
                }
                closedir $dh;
                @ls_files_113 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_files_113;
            }
        }
        (@ls_files_113 ? join("\n", @ls_files_113) . "\n" : q{});
    };
    ;
    if ($CHILD_ERROR != 0) { $pipeline_success_112 = 0; }
    $output_112 = do {
            my $_wc_data = $output_112;
            my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
            my $_wc_result = q{};
            $_wc_result .= sprintf q{%d}, $_wc_lines;
            $_wc_result .= "\n";
            $_wc_result;
        };
    if ( !$pipeline_success_112 ) { $main_exit_code = 1; }
    $output_112 =~ s/\n+\z//msx;
    $output_112;
}; $_pipeline_result; };
do {
    my $output = "File count: $count";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
$current_user = ('root');
if ("$current_user" eq "root") {
    print "Running as root\n";
}
else {
    print "Not running as root\n";
}
my $system_name;
my @system_name;
my %system_name;
$system_name = 'Darwin';
if ($system_name =~ /^Linux$/msx) {
        print "Running on Linux\n";
} elsif ($system_name =~ /^Darwin$/msx) {
        print "Running on macOS\n";
} elsif (1) {
        print "Running on other " . "sys" . "tem\n";
}

sub get_file_size {
    my $file = $_[0];
    my $size = do {
    my $wc_file = "$file";
    my $wc_file_opened = 0;
    my $content = do {
        my $result = q{};
        if (open my $fh, '<', $wc_file) {
            $wc_file_opened = 1;
            local $INPUT_RECORD_SEPARATOR = undef;
            $result = <$fh>;
            close $fh or warn "Close failed: $OS_ERROR\n";
        } else {
            warn "Cannot open $wc_file: $OS_ERROR\n";
        }
        $result;
    };
    $wc_file_opened ? do {
        my $wc_bytes = length($content);
        $wc_bytes;
    } : q{};
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
my @files = (do { my $_result = `ls -1 *.sh examples/*.sh 2>/dev/null`; chomp $_result; $CHILD_ERROR = $? >> 8; split("\n", $_result); });
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
my $process_result;
my @process_result;
my %process_result;
$process_result = do { my $command = "bash -c 'comm -23 <(sort file1.txt) <(sort file2.txt)'"; chomp(my $result = qx{$command}); $CHILD_ERROR = $? >> 8; $result; };
print "Process substitution result:\n";
print $process_result;
if ( !( ($process_result) =~ m{\n\z}msx ) ) { print "\n"; }
my $here_string_result;
my @here_string_result;
my %here_string_result;
$here_string_result = do { my $input_data = "hello world"; my $set1_116 = 'a-z';
my $set2_116 = 'A-Z';
my $input_116 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_116 = $set1_116;
my $expanded_set2_116 = $set2_116;
# Handle a-z range in set1
if ($expanded_set1_116 =~ /a-z/msx) {
    $expanded_set1_116 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_116 =~ /A-Z/msx) {
    $expanded_set1_116 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_116 =~ /\[:upper:\]/msx) {
    $expanded_set1_116 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_116 =~ /\[:lower:\]/msx) {
    $expanded_set1_116 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_116 =~ /a-z/msx) {
    $expanded_set2_116 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_116 =~ /A-Z/msx) {
    $expanded_set2_116 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_116 =~ /\[:upper:\]/msx) {
    $expanded_set2_116 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_116 =~ /\[:lower:\]/msx) {
    $expanded_set2_116 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_115 = q{};
for my $char ( split //msx, $input_116 ) {
    my $pos_116 = index $expanded_set1_116, $char;
    if ( $pos_116 >= 0 && $pos_116 < length $expanded_set2_116 ) {
        $tr_result_115 .= substr $expanded_set2_116, $pos_116, 1;
    } else {
        $tr_result_115 .= $char;
    }
}
$tr_result_115 };
do {
    my $output = "Here string result: $here_string_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $perl_result;
my @perl_result;
my %perl_result;
$perl_result = do {
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

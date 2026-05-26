#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use Digest::SHA   qw(sha256_hex sha512_hex);
use File::Path    qw(make_path remove_tree);
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
our $CHILD_ERROR;

print "=== Output and Formatting Commands ===\n";
my $echo_result = do {
    my $_chomp_temp = ("Hello from backticks");
    chomp $_chomp_temp;
    $_chomp_temp;
};
do {
    my $output = "Echo result: $echo_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $printf_result = do {
    my $result = sprintf "Number: %d, String: %s\n", '42', "test";
    $result;
};
do {
    my $output = "Printf result: $printf_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
print "=== Compression Commands ===\n";
print "=== Network Commands ===\n";
print "=== Process Management Commands ===\n";
print "=== Checksum Commands ===\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'test_checksum.txt'
      or die "Cannot open file: $OS_ERROR\n";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};
my $sha256_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha256_hex(
            do {
                local $INPUT_RECORD_SEPARATOR = undef;
                open my $fh, '<', 'test_checksum.txt'
                  or croak "Cannot open 'test_checksum.txt': $ERRNO";
                my $content = <$fh>;
                close $fh
                  or croak "Close failed: $ERRNO";
                $content;
            }
        );
        push @results, "$hash  test_checksum.txt";
    }
    else {
        push @results,
"0000000000000000000000000000000000000000000000000000000000000000  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
do {
    my $output = "SHA256 result: $sha256_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $sha512_result = do {
    my @results;
    if ( -f 'test_checksum.txt' ) {
        my $hash = sha512_hex(
            do {
                local $INPUT_RECORD_SEPARATOR = undef;
                open my $fh, '<', 'test_checksum.txt'
                  or croak "Cannot open 'test_checksum.txt': $ERRNO";
                my $content = <$fh>;
                close $fh
                  or croak "Close failed: $ERRNO";
                $content;
            }
        );
        push @results, "$hash  test_checksum.txt";
    }
    else {
        push @results,
"00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000  test_checksum.txt  FAILED open or read";
    }
    join("\n", @results) . "\n";
};
do {
    my $output = "SHA512 result: $sha512_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $strings_result = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    my $input_data;
    if ( open my $fh, '<', 'test_binary.txt' ) {
        local $INPUT_RECORD_SEPARATOR = undef;    # Read entire file at once
        $input_data = <$fh>;
        close $fh
          or croak "Close failed: $ERRNO";
    }
    else {
        print {*STDERR} "strings: 'test_binary.txt': No such file\n";
        $input_data = q{};
    }
    my @result;
    while ($input_data =~ /([\x20-\x7E]{4,})/g) {
        push @result, $1;
    }
    my $line = join "\n", @result;
    $output_0 = $line;
    my $num_lines       = 3;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_0;
    my $pos             = 0;

    while ( $pos < length $input && $head_line_count < $num_lines ) {
        my $line_end = index $input, "\n", $pos;
        if ( $line_end == -1 ) {
            $line_end = length $input;
        }
        my $head_line = substr $input, $pos, $line_end - $pos;
        $result .= $head_line . "\n";
        $pos = $line_end + 1;
        ++$head_line_count;
    }
    $output_0 = $result;

    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    $output_0 =~ s/\n+\z//msx;
    $output_0;
} };
print "Strings result:\n";
print $strings_result;
if ( !( $strings_result =~ m{\n\z}msx ) ) { print "\n"; }
print "=== I/O Redirection Commands ===\n";
my $tee_result = do { do {
    my $output_1 = q{};
    my $output_printed_1;
    my $pipeline_success_1 = 1;
    $output_1 .= 'test output' . "\n";
    if ( !($output_1 =~ m{\n\z}msx) ) { $output_1 .= "\n"; }
    $CHILD_ERROR = 0;
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee.txt' ) {
        print {$fh} $output_1;
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        carp "tee: Cannot open 'test_tee.txt': $ERRNO";
    }
    $output_1 = $output_1;
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
    $output_1 =~ s/\n+\z//msx;
    $output_1;
} };
do {
    my $output = "Tee result: $tee_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
print "=== Perl Command ===\n";
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
if ( -e "test_checksum.txt" ) {
    if ( -d "test_checksum.txt" ) {
        carp "rm: carping: ", "test_checksum.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_checksum.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_checksum.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
if ( -e "test_tee.txt" ) {
    if ( -d "test_tee.txt" ) {
        carp "rm: carping: ", "test_tee.txt",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "test_tee.txt" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_tee.txt",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

exit $main_exit_code;

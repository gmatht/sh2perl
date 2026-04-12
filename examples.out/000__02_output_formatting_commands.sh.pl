#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;
use Digest::SHA   qw(sha256_hex sha512_hex);
use File::Path    qw(make_path remove_tree);
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

print "=== Output and Formatting Commands ===\n";
my $echo_result = do {
    my $_chomp_temp = ('Hello from backticks');
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
my $printf_result = do {
    my $result = sprintf "Number: %d, String: %s\n", "42", "test";
    $result;
};
do {
    my $output = "Printf result: $printf_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
print "=== Compression Commands ===\n";
print "=== Network Commands ===\n";
print "=== Process Management Commands ===\n";
print "=== Checksum Commands ===\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'test_checksum.txt'
      or die "Cannot open file: $!\n";
    print "test content\n";
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
      or die "Close failed: $!\n";
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
    join "\n", @results;
};
do {
    my $output = "SHA256 result: $sha256_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
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
    join "\n", @results;
};
do {
    my $output = "SHA512 result: $sha512_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
my $strings_result = do {
    my $output_3;
    my $pipeline_success_3 = 1;
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
    my @lines = split /\n/msx, $input_data;
    for my $line (@lines) {
        if ( length $line >= 4 ) {
            push @result, $line;
        }
    }
    my $line = join "\n", @result;
    $output_3 = $line;
    my $num_lines       = 3;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_3;
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
    $output_3 = $result;

    if ( !$pipeline_success_3 ) { $main_exit_code = 1; }
    $output_3 =~ s/\n+\z//msx;
    $output_3;
};
print "Strings result:\n";
print $strings_result;
if ( !( $strings_result =~ m{\n\z}msx ) ) { print "\n"; }
print "=== I/O Redirection Commands ===\n";
my $tee_result = do {
    my $output_4;
    my $pipeline_success_4 = 1;
    $output_4 .= "test output\n";
    if ( !($output_4 =~ m{\n\z}msx) ) { $output_4 .= "\n"; }
    my @lines = split /\n/msx, $output_4;
    if ( open my $fh, '>', 'test_tee.txt' ) {
        foreach my $line (@lines) {
            print {$fh} "$line\n";
        }
        close $fh
          or croak "Close failed: $ERRNO";
    }
    else {
        carp "tee: Cannot open 'test_tee.txt': $ERRNO";
    }
    if ( !$pipeline_success_4 ) { $main_exit_code = 1; }
    $output_4 =~ s/\n+\z//msx;
    $output_4;
};
do {
    my $output = "Tee result: $tee_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
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
    carp "rm: carping: ", "test_checksum.txt", ": No such file or directory\n";
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
    carp "rm: carping: ", "test_tee.txt", ": No such file or directory\n";
}

exit $main_exit_code;

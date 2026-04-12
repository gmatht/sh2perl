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

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

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
    my $output_114;
    my $pipeline_success_114 = 1;
    my $input_data;
    if ( open my $fh, '<', 'target/debug/debashc.exe' ) {
        local $INPUT_RECORD_SEPARATOR = undef;    # Read entire file at once
        $input_data = <$fh>;
        close $fh
          or croak "Close failed: $ERRNO";
    }
    else {
        print {*STDERR} "strings: 'target/debug/debashc.exe': No such file\n";
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
    $output_114 = $line;
    my $num_lines       = 3;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_114;
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
    $output_114 = $result;

    if ( !$pipeline_success_114 ) { $main_exit_code = 1; }
    $output_114 =~ s/\n+\z//msx;
    $output_114;
};
print "Strings result:\n";
print $strings_result;
if ( !( $strings_result =~ m{\n\z}msx ) ) { print "\n"; }
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
print "=== Checksum Commands Complete ===\n";

exit $main_exit_code;

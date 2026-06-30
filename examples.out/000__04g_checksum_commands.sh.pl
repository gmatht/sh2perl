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
    0;
};
my $sha256_result;
$sha256_result = do {
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
;
do {
    my $output = "SHA256 result: $sha256_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $sha512_result;
$sha512_result = do {
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
;
do {
    my $output = "SHA512 result: $sha512_result";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
my $strings_result;
$strings_result = do { do {
    do { my $output_112 = q{};
my $output_printed_112;
my $head_line_count = 0;
my $output_113 = q{};
while (my $line = <>) {
    chomp $line;
    my @filenames = ('target/debug/debashc.exe');
my $combined_output = q{};
foreach my $filename (@filenames) {
my $input_data;
if ( open my $fh, '<', $filename ) {
    local $INPUT_RECORD_SEPARATOR = undef;
    $input_data = <$fh>;
    close $fh
      or croak "Close failed: $ERRNO";
}
else {
    $combined_output .= "strings: '$filename': No such file\n";
    $input_data = q{};
}
my @result;
while ($input_data =~ /([\x20-\x7E]{4,})/g) {
    push @result, $1;
}
my $line = join "\n", @result;
$line .= "\n" if $line ne q{};
$combined_output .= $line;
}
$output_113 = $combined_output;
    if ($head_line_count < 3) {
    $output_0 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
} };
} };
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
    $CHILD_ERROR = 0;
}
print "=== Checksum Commands Complete ===\n";

exit $main_exit_code;

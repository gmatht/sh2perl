#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use File::Basename;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

$PROGRAM_NAME = '063_hard_to_parse.sh';
my $var;
my @var;
my %var;
my $files;
my @files;
my %files;
my $file;
my @file;
my %file;
my $line;
my @line;
my %line;
my $dir;
my @dir;
my %dir;
my $array;
my @array;
my %array;

my $result;
my @result;
my %result;
my $a;
my $b;
my $c;
my $d;
my $e;
my $f;
my $g;
my $h;
my $i;
my $j;
my $k;
my $l;
my $m;
my $n;
$result = eval { int( ($a + $b) * ($c - $d) / ($e % $f) + ($g ** $h) - ($i << $j) | ($k & $l) ^ ($m | $n) ) } // "";
my %matrix = ();
my $x;
my $y;
my $z;
$matrix{"0,0"} = eval { int( ($x + $y) * $z ) } // "";
$matrix{"1,1"} = $array[eval { int($ENV{index}) } // ""];
$matrix{"2,2"} = q{};
$matrix{"3,3"} = scalar(@array);
my $output = do {
    my $_chomp_temp = ("Result: " . (do { my $_chomp_temp = ("Nested: " . (do { my $_chomp_temp = ("Deep: " . (do { my $_chomp_temp = ("Level 4"); chomp $_chomp_temp; $_chomp_temp; })); chomp $_chomp_temp; $_chomp_temp; })); chomp $_chomp_temp; $_chomp_temp; }));
    chomp $_chomp_temp;
    $_chomp_temp;
};
do {
    my $output = (defined ${var} && ${var} ne q{} ? ${var} : (defined ($ENV{default} // q{}) && ($ENV{default} // q{}) ne q{} ? ($ENV{default} // q{}) : (defined ($ENV{fallback} // q{}) && ($ENV{fallback} // q{}) ne q{} ? ($ENV{fallback} // q{}) : do { my $_result = ("computed"); $_result; })));
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
do {
    my $output = (defined $ENV{'array[${index}]'} && $ENV{'array[${index}]'} ne q{} ? $ENV{'array[${index}]'} : @main::default[0..2]);
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
$CHILD_ERROR = 0;
print q{} . "\n";
$CHILD_ERROR = 0;
# Original bash: cat << 'EOF' | grep -v "^#" | sed 's/^/  /'
{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        my $output;
    $output = q[# This is a comment
$(echo "Command substitution")
${var:-default}
$(( 1 + 2 * 3 ))
];
    $output_0 = $output;

        my $grep_result_0_1;
    my @grep_lines_0_1 = split /\n/msx, $output_0;
    my @grep_filtered_0_1 = grep { !/^\#/msx } @grep_lines_0_1;
    $grep_result_0_1 = join "\n", @grep_filtered_0_1;
    if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) {
    $grep_result_0_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_0_1 > 0 ? 0 : 1;
    $output_0 = $grep_result_0_1;
    $output_0 = $grep_result_0_1;

        my @sed_lines_0 = split /\n/msx, $output_0;
    my @sed_result_0;
    foreach my $line (@sed_lines_0) {
    chomp $line;
    $line =~ s/^/  /gmsx;
    push @sed_result_0, $line;
    }
    $output_0 = join "\n", @sed_result_0;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }
if (my $pid = fork()) {
    # Parent process continues
} elsif (defined $pid) {
    # Child process executes the background command
    exec 'bash', '-c', '(sleep 1; echo Starting)';
    croak "exec failed: $OS_ERROR\n";
} else {
    die "Cannot fork: $ERRNO\n";
}
if (my $pid = fork()) {
    # Parent process continues
} elsif (defined $pid) {
    # Child process executes the background command
    exec 'bash', '-c', '(sleep 2; echo Processing)';
    croak "exec failed: $OS_ERROR\n";
} else {
    die "Cannot fork: $ERRNO\n";
}
1 while wait() > -1;
$CHILD_ERROR = $? >> 8;
print "All done\n";
if ((($var =~ /^[0-9]+$/msx && !($main_exit_code = eval { int($var > 0) } // "")) && (-f "$file"))) {
if ((q{} =~ /"value"/msx || !(    $main_exit_code = eval { int(scalar(@array) > 5) } // ""))) {
if ((qx'echo "$var" | grep -q "pattern"' ne q{})) {
            print "Deeply nested condition met\n";
        }
    }
}
if ((do { my $_chomp_temp = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $input_data = ("$var") . "\n";
    my $set1_2 = '[:upper:]';
my $set2_2 = '[:lower:]';
my $input_2 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_2 = $set1_2;
my $expanded_set2_2 = $set2_2;
# Handle a-z range in set1
if ($expanded_set1_2 =~ /a-z/msx) {
    $expanded_set1_2 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_2 =~ /A-Z/msx) {
    $expanded_set1_2 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_2 =~ /\[:upper:\]/msx) {
    $expanded_set1_2 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_2 =~ /\[:lower:\]/msx) {
    $expanded_set1_2 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_2 =~ /a-z/msx) {
    $expanded_set2_2 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_2 =~ /A-Z/msx) {
    $expanded_set2_2 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_2 =~ /\[:upper:\]/msx) {
    $expanded_set2_2 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_2 =~ /\[:lower:\]/msx) {
    $expanded_set2_2 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_1 = q{};
for my $char ( split //msx, $input_2 ) {
    my $pos_2 = index $expanded_set1_2, $char;
    if ( $pos_2 >= 0 && $pos_2 < length $expanded_set2_2 ) {
        $tr_result_1 .= substr $expanded_set2_2, $pos_2, 1;
    } else {
        $tr_result_1 .= $char;
    }
}
$tr_result_1
}; $_pipeline_result; }; chomp $_chomp_temp; $_chomp_temp; }) =~ /^.*\[0-9\].*$/msx) {
    if (lc(lc(${var})) =~ /^.*pattern.*$/msx) {
                print "Double nested pattern\n";
    } elsif (1) {
                print "Single nested pattern\n";
    }
} elsif (1) {
        print "No numbers\n";
}

sub complex_function {
    my @args = (@_);
    my %options = ();
    my $i = "0";
while (     $main_exit_code = eval { int($i < scalar(@args)) } // "" ) {
if ($args[eval { int($i) } // ""] =~ /^--.*$/msx) {
                        my $key = ($args[eval { int($i) } // ""] =~ s/^--//r);
                        my $value = (defined $args[eval { int($i+1) } // ""] && $args[eval { int($i+1) } // ""] ne q{} ? $args[eval { int($i+1) } // ""] : 'true');
                        $options{"$key"} = "$value";
                        $main_exit_code = eval { int($i += 2) } // "";
        } elsif ($args[eval { int($i) } // ""] =~ /^-.*$/msx) {
                        my $flags = ($args[eval { int($i) } // ""] =~ s/^-//r);
                        my $j = "0";
            while (             $main_exit_code = eval { int($j < length($flags)) } // "" ) {
                $options{substr($flags, $j, 1)} = "true";
                $main_exit_code = eval { int($j++) } // "";
            }
                        $main_exit_code = eval { int($i++) } // "";
        } elsif (1) {
            last;        }
    }
    print "Processed " . scalar(keys %options) . " options\n";
    return;
}
for (eval { int($i=0) } // ""; eval { int($i<scalar(@array)) } // ""; eval { int($i++) } // "") {
        for (eval { int($j=0) } // ""; eval { int($j<0) } // ""; eval { int($j++) } // "") {
if (eval { int($array[$i][$j] > $ENV{threshold}) } // "") {
                    $result{"i"} = eval { int( $result[$i] + $array[$i][$j] ) } // "";
                }
        }
}
my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
my $output_ps_fh_1;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
    my $output_3 = q{};
    my $output_printed_3;
    my $head_line_count = 0;
    my $output_5 = q{};
    while (my $line = <>) {
        chomp $line;
            if (!($line =~ /^\#/msx)) {
            next;
        }
        if ($head_line_count < 10) {
        $output_5 .= $line . "\n";
        ++$head_line_count;
    } else {
        $line = q{}; # Clear line to prevent printing
        last; # Break out of the yes loop when head limit is reached
    }
        print $line . "\n";
    }
    $output_5;
}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_1} $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
while (     my $IFS = q{};
    if (do {
if (do {
my $L = <>;
chomp $L;
    $CHILD_ERROR == 0
}) {
    "$line" ne q{}}
        $CHILD_ERROR == 0
    }) {
                $main_exit_code = eval { int($ENV{counter} < $ENV{max_lines}) } // "";
    } ) {
if ("$line" =~ /^[[:space:]]*\#/msx) {
next;
    }
if ("$line" =~ /^.*\$\(.*\).*$/msx) {
                do {
    my $output = "Contains command substitution: $line";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
        $CHILD_ERROR = 0;
    } elsif ("$line" =~ /^.*\$\{\[^}\].*\}.*$/msx) {
                do {
    my $output = "Contains parameter expansion: $line";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
        $CHILD_ERROR = 0;
    } elsif ("$line" =~ /^.*\$\(\(.*\)\).*$/msx) {
                do {
    my $output = "Contains arithmetic expansion: $line";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
        $CHILD_ERROR = 0;
    }
    $main_exit_code = eval { int($ENV{counter}++) } // "";
}
my $index;
$result = eval { int( (defined $var && $var ne q{} ? $var : 0) + (defined $array[(defined $index && $index ne q{} ? $index : 0)] && $array[(defined $index && $index ne q{} ? $index : 0)] ne q{} ? $array[(defined $index && $index ne q{} ? $index : 0)] : 0) ) } // "";
# Original bash: #!/bin/bash
{
    my $output_7 = q{};
    my $output_printed_7;
    my $pipeline_success_7 = 1;
        $output_7 = q{};
    my ($in_8, $out_8, $err_8);
    my $pid_8 = open3($in_8, $out_8, $err_8, 'sh', '-c', q{((echo 'Level 3'; (echo 'Level 4'; echo 'Still level 4')) | grep Level) | sed s/Level/Depth/});
    close $in_8 or croak 'Close failed: $OS_ERROR';
    $output_7 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_8> };
    close $out_8 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_8, 0;

        my $output_7_1 = do {
    my $_wc_data = $output_7;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_lines;
    $_wc_result .= "\n";
    $_wc_result;
    };
    $output_7 = $output_7_1;
    if ($output_7 ne q{} && !defined $output_printed_7) {
        print $output_7;
        if (!($output_7 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_7 ) { $main_exit_code = 1; }
    }
my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
my $output_ps_fh_2;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'sort file1.txt');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_2 = dirname($temp_file_ps_fh_2);
if (!-d $temp_dir_fh_2) { make_path($temp_dir_fh_2); }
open my $fh_ps_fh_2, '>', $temp_file_ps_fh_2 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_2} $output_ps_fh_2;
close $fh_ps_fh_2 or croak "Close failed: $ERRNO\n";
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
my ($in, $out, $err);
my $pid = open3($in, $out, $err, 'bash', '-c', 'sort file2.txt');
close $in or croak 'Close failed: $OS_ERROR';
$output_ps_fh_3 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
close $out or croak 'Close failed: $OS_ERROR';
waitpid $pid, 0;
$CHILD_ERROR = $? >> 8;
}
use File::Path qw(make_path);
my $temp_dir_fh_3 = dirname($temp_file_ps_fh_3);
if (!-d $temp_dir_fh_3) { make_path($temp_dir_fh_3); }
open my $fh_ps_fh_3, '>', $temp_file_ps_fh_3 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_3} $output_ps_fh_3;
close $fh_ps_fh_3 or croak "Close failed: $ERRNO\n";
local *STDERR;
open STDERR, '>', q{1} or croak "Cannot open file: $OS_ERROR\n";
$ENV{DIFF_TEMP_FILE1} = q{/tmp} . '/process_sub_fh_2.tmp';
$ENV{DIFF_TEMP_FILE2} = q{/tmp} . '/process_sub_fh_3.tmp';
my $diff_exit_code = 0;
my $diff_output = q{};
{
    my $diff_cmd = 'diff';
    my @diff_args = ($temp_file_ps_fh_2, $temp_file_ps_fh_3);
    my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
    if ($diff_pid) {
        local $INPUT_RECORD_SEPARATOR = undef;
        $diff_output = <$diff_fh>;
        close $diff_fh;
        $diff_exit_code = $? >> 8;
    } else {
        carp "Cannot execute diff command: $OS_ERROR";
        $diff_output = q{};
        $diff_exit_code = 1;
    }
}
print $diff_output;

sub define_complex_function {
    my $name = "$_[0]";
    my $body = "$_[1]";
do { my $eval_input = $name . "() {\n        " . $body . "\n    }"; system('bash', '-c', "eval \"$eval_input\""); $CHILD_ERROR = $? >> 8; };
    return;
}
if ((("$var" ne q{} && ((-f "$file") || (-d "$dir"))) && ("$(wc -l < "$file")" > 10))) {
    print "Complex test passed\n";
}
print join(q[ ], ('a' . '1' . 'x', 'a' . '1' . 'y', 'a' . '1' . 'z', 'a' . '2' . 'x', 'a' . '2' . 'y', 'a' . '2' . 'z', 'a' . '3' . 'x', 'a' . '3' . 'y', 'a' . '3' . 'z', 'b' . '1' . 'x', 'b' . '1' . 'y', 'b' . '1' . 'z', 'b' . '2' . 'x', 'b' . '2' . 'y', 'b' . '2' . 'z', 'b' . '3' . 'x', 'b' . '3' . 'y', 'b' . '3' . 'z', 'c' . '1' . 'x', 'c' . '1' . 'y', 'c' . '1' . 'z', 'c' . '2' . 'x', 'c' . '2' . 'y', 'c' . '2' . 'z', 'c' . '3' . 'x', 'c' . '3' . 'y', 'c' . '3' . 'z')) . "\n";
$CHILD_ERROR = 0;
my $here_string_content_fh_4 = (do { my $_chomp_temp = ("UPPER: " . uc(uc(${var}))); chomp $_chomp_temp; $_chomp_temp; });
my $set1_9 = '[:upper:]';
my $set2_9 = '[:lower:]';
my $input_9 = $here_string_content_fh_4;
# Expand character ranges for tr command
my $expanded_set1_9 = $set1_9;
my $expanded_set2_9 = $set2_9;
# Handle a-z range in set1
if ($expanded_set1_9 =~ /a-z/msx) {
    $expanded_set1_9 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_9 =~ /A-Z/msx) {
    $expanded_set1_9 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_9 =~ /\[:upper:\]/msx) {
    $expanded_set1_9 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_9 =~ /\[:lower:\]/msx) {
    $expanded_set1_9 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_9 =~ /a-z/msx) {
    $expanded_set2_9 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_9 =~ /A-Z/msx) {
    $expanded_set2_9 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_9 =~ /\[:upper:\]/msx) {
    $expanded_set2_9 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_9 =~ /\[:lower:\]/msx) {
    $expanded_set2_9 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_0 = q{};
for my $char ( split //msx, $input_9 ) {
    my $pos_9 = index $expanded_set1_9, $char;
    if ( $pos_9 >= 0 && $pos_9 < length $expanded_set2_9 ) {
        $tr_result_0 .= substr $expanded_set2_9, $pos_9, 1;
    } else {
        $tr_result_0 .= $char;
    }
}    print $tr_result_0;
complex_function("\\");
$main_exit_code = system('--long-option="value', 'with', "spaces\\") >> 8;
$main_exit_code = system('--array-option', "item1", "item2", "item3", "\\") >> 8;
$main_exit_code = system('--flag', "\\") >> 8;
$main_exit_code = system('positional argument', "\\") >> 8;
$main_exit_code = system('unknown_command', "\\") >> 8;
{
    my $output_10 = q{};
    my $output_printed_10;
    my $pipeline_success_10 = 1;
        $output_10 = q{};
    my ($in_11, $out_11, $err_11);
    my $pid_11 = open3($in_11, $out_11, $err_11, 'sh', '-c', 'echo "Complex command cannot be converted to shell command"');
    close $in_11 or croak 'Close failed: $OS_ERROR';
    $output_10 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_11> };
    close $out_11 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_11, 0;

        my @sort_lines_10_1 = split /\n/msx, $output_10;
    my @sort_sorted_10_1 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_10_1;
    my $output_10_1 = join "\n", @sort_sorted_10_1;
    if ($output_10_1 ne q{} && !($output_10_1 =~ m{\n\z}msx)) {
    $output_10_1 .= "\n";
    }
    $output_10 = $output_10_1;
    $output_10 = $output_10_1;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'final_result.txt'
    or die "Cannot open file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_12 = q{};
    my @lines = split /\n/msx, $output_10;
    my $num_lines = 5;
    if ($num_lines > scalar @lines) {
    $num_lines = scalar @lines;
    }
    my $start_index = scalar @lines - $num_lines;
    if ($start_index < 0) { $start_index = 0; }
    my @result = @lines[$start_index..$#lines];
    $output_10 = join "\n", @result;
    if ($output_10 ne q{} && !($output_10  =~ m{\n\z}msx)) { $output_10 .= "\n"; }
    $tmp_redirect_12;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_10; }
    $output_printed_10 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_10 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

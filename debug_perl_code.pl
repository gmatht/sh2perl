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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '063_hard_to_parse.sh';
my $array;
my @array;
my %array;
my $var;
my @var;
my %var;
my $file;
my @file;
my %file;
my $files;
my @files;
my %files;
my $line;
my @line;
my %line;
my $dir;
my @dir;
my %dir;

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
$matrix{"0,0"} = eval { int( ($ENV{x} + $ENV{y}) * $ENV{z} ) } // "";
$matrix{"1,1"} = $array[eval { int($ENV{index}) } // ""];
$matrix{"2,2"} = q{};
$matrix{"3,3"} = scalar(@array);
my $output;
my @output;
my %output;
$output = ("Result: " . (do { my $_chomp_temp = ("Nested: " . (do { my $_chomp_temp = ("Deep: " . (do { my $_chomp_temp = ("Level 4"); chomp $_chomp_temp; $_chomp_temp; })); chomp $_chomp_temp; $_chomp_temp; })); chomp $_chomp_temp; $_chomp_temp; }));
do {
    my $__echo_line = (defined ${var} && ${var} ne q{} ? ${var} : (defined ($ENV{default} // q{}) && ($ENV{default} // q{}) ne q{} ? ($ENV{default} // q{}) : (defined ($ENV{fallback} // q{}) && ($ENV{fallback} // q{}) ne q{} ? ($ENV{fallback} // q{}) : do { my $_result = ("computed"); $_result; })));
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
do {
    my $__echo_line = (defined $ENV{'array[${index}]'} && $ENV{'array[${index}]'} ne q{} ? $ENV{'array[${index}]'} : @main::default[0..2]);
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
$CHILD_ERROR = 0;
$CHILD_ERROR = 0;
# Original bash: cat << 'EOF' | grep -v "^#" | sed 's/^/  /'
{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output = q{};
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
    do {
        local %ENV = %ENV;
        my $m = $m;
        my $d = $d;
        my $i = $i;
        my $j = $j;
        my $output = $output;
        my $h = $h;
        my $n = $n;
        my $f = $f;
        my %matrix = %matrix;
        my $l = $l;
        my $g = $g;
        my $c = $c;
        my $b = $b;
        my $k = $k;
        my $result = $result;
        my $e = $e;
        my $a = $a;
require Time::HiRes; Time::HiRes::sleep(q{1});
            print "Starting\n";
        q{};
    };
    exit(0);
} else {
    die "Cannot fork: $ERRNO\n";
}
if (my $pid = fork()) {
    # Parent process continues
} elsif (defined $pid) {
    # Child process executes the background command
    do {
        local %ENV = %ENV;
        my $m = $m;
        my $d = $d;
        my $i = $i;
        my $j = $j;
        my $output = $output;
        my $h = $h;
        my $n = $n;
        my $f = $f;
        my %matrix = %matrix;
        my $l = $l;
        my $g = $g;
        my $c = $c;
        my $b = $b;
        my $k = $k;
        my $result = $result;
        my $e = $e;
        my $a = $a;
require Time::HiRes; Time::HiRes::sleep(q{2});
            print "Processing\n";
        q{};
    };
    exit(0);
} else {
    die "Cannot fork: $ERRNO\n";
}
1 while wait() > -1;
$CHILD_ERROR = $? == -1 ? 0 : $? >> 8;
print "All done\n";
if ((($var =~ /^[0-9]+$/msx && !($CHILD_ERROR = ($main_exit_code = eval { int($var > 0) } // "") ? 0 : 1)) && (-f "$file"))) {
if ((q{} =~ /"value"/msx || !(    $CHILD_ERROR = ($main_exit_code = eval { int(scalar(@array) > 5) } // "") ? 0 : 1))) {
if ((qx'echo "$var" | grep -q "pattern"' ne q{})) {
            print "Deeply nested condition met\n";
        }
    }
}
if ((do { my $_chomp_temp = do { local $CHILD_ERROR = 0; my $_pipeline_result = do {
    my $input_data = ("$var") . "\n";
    my $set1_4 = '[:upper:]';
my $set2_4 = '[:lower:]';
my $input_4 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_4 = $set1_4;
my $expanded_set2_4 = $set2_4;
# Handle a-z range in set1
if ($expanded_set1_4 =~ /a-z/msx) {
    $expanded_set1_4 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_4 =~ /A-Z/msx) {
    $expanded_set1_4 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_4 =~ /\[:upper:\]/msx) {
    $expanded_set1_4 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_4 =~ /\[:lower:\]/msx) {
    $expanded_set1_4 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_4 =~ /a-z/msx) {
    $expanded_set2_4 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_4 =~ /A-Z/msx) {
    $expanded_set2_4 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_4 =~ /\[:upper:\]/msx) {
    $expanded_set2_4 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_4 =~ /\[:lower:\]/msx) {
    $expanded_set2_4 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_3 = q{};
for my $char ( split //msx, $input_4 ) {
    my $pos_4 = index $expanded_set1_4, $char;
    if ( $pos_4 >= 0 && $pos_4 < length $expanded_set2_4 ) {
        $tr_result_3 .= substr $expanded_set2_4, $pos_4, 1;
    } else {
        $tr_result_3 .= $char;
    }
}
$tr_result_3
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
while ( !(    $CHILD_ERROR = ($main_exit_code = eval { int($i < scalar(@args)) } // "") ? 0 : 1) ) {
if ($args[eval { int($i) } // ""] =~ /^--.*$/msx) {
                        my $key = ($args[eval { int($i) } // ""] =~ s/^--//r);
                        my $value = (defined $args[eval { int($i+1) } // ""] && $args[eval { int($i+1) } // ""] ne q{} ? $args[eval { int($i+1) } // ""] : 'true');
                        $options{"$key"} = "$value";
                        $CHILD_ERROR = ($main_exit_code = eval { int($i += 2) } // "") ? 0 : 1;
        } elsif ($args[eval { int($i) } // ""] =~ /^-.*$/msx) {
                        my $flags = ($args[eval { int($i) } // ""] =~ s/^-//r);
                        my $j = "0";
            while ( !(            $CHILD_ERROR = ($main_exit_code = eval { int($j < length($flags)) } // "") ? 0 : 1) ) {
                $options{substr($flags, $j, 1)} = "true";
                $CHILD_ERROR = ($main_exit_code = eval { int($j++) } // "") ? 0 : 1;
            }
                        $CHILD_ERROR = ($main_exit_code = eval { int($i++) } // "") ? 0 : 1;
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
    my $output_5 = q{};
    my $output_printed_5;
    my $head_line_count = 0;
    my $output_7 = q{};
    while (my $line = <>) {
        chomp $line;
            if (!($line =~ /^\#/msx)) {
            next;
        }
        if ($head_line_count < 10) {
        $output_7 .= $line . "\n";
        ++$head_line_count;
    } else {
        $line = q{}; # Clear line to prevent printing
        last; # Break out of the yes loop when head limit is reached
    }
        print $line . "\n";
    }
    $output_7;
}
use File::Path qw(make_path);
my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_1} $output_ps_fh_1;
close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_1 or croak "Cannot open process substitution: $ERRNO\n";
my $line;
while (1) {
    my $IFS = q{};
    last unless $CHILD_ERROR == 0;
    last unless do {
        $line = <>;
        chomp $line;
        $CHILD_ERROR = defined($line) ? 0 : 1;
        $CHILD_ERROR == 0
    };
    last unless ("$line" ne q{});
    last unless do {
        $CHILD_ERROR = ($main_exit_code = eval { int($ENV{counter} < $ENV{max_lines}) } // "") ? 0 : 1;
        $CHILD_ERROR == 0
    };
if ("$line" =~ /^[[:space:]]*\#/msx) {
next;
    }
if ("$line" =~ /^.*\$\(.*\).*$/msx) {
                do {
    my $__echo_line = "Contains command substitution: $line";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
        $CHILD_ERROR = 0;
    } elsif ("$line" =~ /^.*\$\{\[^}\].*\}.*$/msx) {
                do {
    my $__echo_line = "Contains parameter expansion: $line";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
        $CHILD_ERROR = 0;
    } elsif ("$line" =~ /^.*\$\(\(.*\)\).*$/msx) {
                do {
    my $__echo_line = "Contains arithmetic expansion: $line";
    print $__echo_line;
    if ( !( $__echo_line =~ m{\n\z}msx ) ) {
        print "\n";
        $__echo_line .= "\n";
    }
    $output .= $__echo_line;
};
        $CHILD_ERROR = 0;
    }
    $CHILD_ERROR = ($main_exit_code = eval { int($ENV{counter}++) } // "") ? 0 : 1;
}
my $index;
$result = eval { int( (defined $var && $var ne q{} ? $var : 0) + (defined $array[(defined $index && $index ne q{} ? $index : 0)] && $array[(defined $index && $index ne q{} ? $index : 0)] ne q{} ? $array[(defined $index && $index ne q{} ? $index : 0)] : 0) ) } // "";
# Original bash: #!/bin/bash
{
    my $output_9 = q{};
    my $output_printed_9;
    my $pipeline_success_9 = 1;
        $output_9 = q{};
    my @_pcmd_11 = ('sh', '-c', q{((echo 'Level 3'; (echo 'Level 4'; echo 'Still level 4')) | grep Level) | sed s/Level/Depth/});
    my ($in_10, $out_10);
    my $pid_10 = open3($in_10, $out_10, '>&STDERR', @_pcmd_11);
    close $in_10 or croak 'Close failed: $OS_ERROR';
    $output_9 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_10> };
    close $out_10 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_10, 0;

        my $output_9_1 = do {
    my $_wc_data = $output_9;
    my $_wc_lines = () = $_wc_data =~ /\n/gsxm;
    my $_wc_result = q{};
    $_wc_result .= sprintf q{%d}, $_wc_lines;
    $_wc_result .= "\n";
    $_wc_result;
    };
    $output_9 = $output_9_1;
    if ($output_9 ne q{} && !defined $output_printed_9) {
        print $output_9;
        if (!($output_9 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_9 ) { $main_exit_code = 1; }
    }
my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
my $output_ps_fh_2;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_2 or croak "Cannot redirect STDOUT";
    my $output_12 = q{};
    my $output_printed_12;
    my $file_content_13 = do {
        local $INPUT_RECORD_SEPARATOR = undef;
        if (open my $fh, '<', 'file1.txt') {
            my $content = <$fh>;
            close $fh or warn "Close failed: $OS_ERROR";
            $content;
        } else {
            warn "Cannot open file: $OS_ERROR";
            q{};
        }
    };
    my @sort_lines_13 = split /\n/msx, $file_content_13;
    my @sort_sorted_13 = sort @sort_lines_13;
    my $sort_output_13 = join "\n", @sort_sorted_13;
    if ($sort_output_13 ne q{} && !($sort_output_13 =~ m{\n\z}msx)) {
        $sort_output_13 .= "\n";
    }
    $file_content_13 = $sort_output_13;
    $output_12 = $sort_output_13;
if ($output_12 ne q{} && !$output_printed_12) {
    print $output_12;
}
}
use File::Path qw(make_path);
my $temp_dir_fh_2 = dirname($temp_file_ps_fh_2);
if (!-d $temp_dir_fh_2) { make_path($temp_dir_fh_2); }
open my $fh_ps_fh_2, '>', $temp_file_ps_fh_2 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_2} $output_ps_fh_2;
close $fh_ps_fh_2 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_2 or croak "Cannot open process substitution: $ERRNO\n";
my $temp_file_ps_fh_3 = q{/tmp} . '/process_sub_fh_3.tmp';
my $output_ps_fh_3;
{
    local *STDOUT;
    open STDOUT, '>', \$output_ps_fh_3 or croak "Cannot redirect STDOUT";
    my $output_14 = q{};
    my $output_printed_14;
    my $file_content_15 = do {
        local $INPUT_RECORD_SEPARATOR = undef;
        if (open my $fh, '<', 'file2.txt') {
            my $content = <$fh>;
            close $fh or warn "Close failed: $OS_ERROR";
            $content;
        } else {
            warn "Cannot open file: $OS_ERROR";
            q{};
        }
    };
    my @sort_lines_15 = split /\n/msx, $file_content_15;
    my @sort_sorted_15 = sort @sort_lines_15;
    my $sort_output_15 = join "\n", @sort_sorted_15;
    if ($sort_output_15 ne q{} && !($sort_output_15 =~ m{\n\z}msx)) {
        $sort_output_15 .= "\n";
    }
    $file_content_15 = $sort_output_15;
    $output_14 = $sort_output_15;
if ($output_14 ne q{} && !$output_printed_14) {
    print $output_14;
}
}
use File::Path qw(make_path);
my $temp_dir_fh_3 = dirname($temp_file_ps_fh_3);
if (!-d $temp_dir_fh_3) { make_path($temp_dir_fh_3); }
open my $fh_ps_fh_3, '>', $temp_file_ps_fh_3 or croak "Cannot create temp file: $ERRNO\n";
print {$fh_ps_fh_3} $output_ps_fh_3;
close $fh_ps_fh_3 or croak "Close failed: $ERRNO\n";
open STDIN, '<', $temp_file_ps_fh_3 or croak "Cannot open process substitution: $ERRNO\n";
do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'comparison.txt'
      or die "Cannot open file: $OS_ERROR\n";
    local *STDERR;
    open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
    $ENV{DIFF_TEMP_FILE1} = q{/tmp} . '/process_sub_fh_2.tmp';
    $ENV{DIFF_TEMP_FILE2} = q{/tmp} . '/process_sub_fh_3.tmp';
    my $diff_output = q{};
    {
        my $diff_cmd = 'diff';
        my @diff_args = ($temp_file_ps_fh_2, $temp_file_ps_fh_3);
        my $diff_pid = open my $diff_fh, q{-|}, $diff_cmd, @diff_args;
        if ($diff_pid) {
            local $INPUT_RECORD_SEPARATOR = undef;
            $diff_output = <$diff_fh>;
            close $diff_fh;
            $CHILD_ERROR = $? >> 8;
        } else {
            carp "Cannot execute diff command: $OS_ERROR";
            $diff_output = q{};
            $CHILD_ERROR = 1;
        }
    }
    print $diff_output;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
};

sub define_complex_function {
    my $name = "$_[0]";
    my $body = "$_[1]";
do { my $eval_input = $name . "() {\n        " . $body . "\n    }"; system('bash', '-c', "eval \"$eval_input\""); $CHILD_ERROR = $? >> 8; };
    return;
}
if ((("$var" ne q{} && ((-f "$file") || (-d "$dir"))) && (qx'wc -l < "$file"' > 10))) {
    print "Complex test passed\n";
}
print join(q[ ], ('a' . '1' . 'x', 'a' . '1' . 'y', 'a' . '1' . 'z', 'a' . '2' . 'x', 'a' . '2' . 'y', 'a' . '2' . 'z', 'a' . '3' . 'x', 'a' . '3' . 'y', 'a' . '3' . 'z', 'b' . '1' . 'x', 'b' . '1' . 'y', 'b' . '1' . 'z', 'b' . '2' . 'x', 'b' . '2' . 'y', 'b' . '2' . 'z', 'b' . '3' . 'x', 'b' . '3' . 'y', 'b' . '3' . 'z', 'c' . '1' . 'x', 'c' . '1' . 'y', 'c' . '1' . 'z', 'c' . '2' . 'x', 'c' . '2' . 'y', 'c' . '2' . 'z', 'c' . '3' . 'x', 'c' . '3' . 'y', 'c' . '3' . 'z')) . "\n";
$CHILD_ERROR = 0;
my $here_string_content_fh_4 = (do { my $_chomp_temp = ("UPPER: " . uc(uc(${var}))); chomp $_chomp_temp; $_chomp_temp; });
my $set1_16 = '[:upper:]';
my $set2_16 = '[:lower:]';
my $input_16 = $here_string_content_fh_4;
# Expand character ranges for tr command
my $expanded_set1_16 = $set1_16;
my $expanded_set2_16 = $set2_16;
# Handle a-z range in set1
if ($expanded_set1_16 =~ /a-z/msx) {
    $expanded_set1_16 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_16 =~ /A-Z/msx) {
    $expanded_set1_16 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_16 =~ /\[:upper:\]/msx) {
    $expanded_set1_16 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_16 =~ /\[:lower:\]/msx) {
    $expanded_set1_16 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_16 =~ /a-z/msx) {
    $expanded_set2_16 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_16 =~ /A-Z/msx) {
    $expanded_set2_16 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_16 =~ /\[:upper:\]/msx) {
    $expanded_set2_16 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_16 =~ /\[:lower:\]/msx) {
    $expanded_set2_16 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_0 = q{};
for my $char ( split //msx, $input_16 ) {
    my $pos_16 = index $expanded_set1_16, $char;
    if ( $pos_16 >= 0 && $pos_16 < length $expanded_set2_16 ) {
        $tr_result_0 .= substr $expanded_set2_16, $pos_16, 1;
    } else {
        $tr_result_0 .= $char;
    }
}    print $tr_result_0;
    if (!($tr_result_0 =~ m{\n\z}msx || $tr_result_0 eq q{})) {
        print "\n";
    }
complex_function('--long-option=', "value with spaces", '--array-option', "item1", "item2", "item3", '--flag', "positional argument", (defined (defined ${var} && ${var} ne q{} ? ${var} : 'default') && (defined ${var} && ${var} ne q{} ? ${var} : 'default') ne q{} ? (defined ${var} && ${var} ne q{} ? ${var} : 'default') : 'default'), (do { my $_chomp_temp = ("computed"); chomp $_chomp_temp; $_chomp_temp; }));
{
    my $output_17 = q{};
    my $output_printed_17;
    my $pipeline_success_17 = 1;
        $output_17 = q{};
    my @_pcmd_19 = ('sh', '-c', ': "Complex command cannot be converted to shell command"');
    my ($in_18, $out_18);
    my $pid_18 = open3($in_18, $out_18, '>&STDERR', @_pcmd_19);
    close $in_18 or croak 'Close failed: $OS_ERROR';
    $output_17 .= do { local $INPUT_RECORD_SEPARATOR = undef; <$out_18> };
    close $out_18 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_18, 0;

        my @sort_lines_17_1 = split /\n/msx, $output_17;
    my @sort_sorted_17_1 = sort {
    my @a_fields = split /\s+/msx, $a;
    my @b_fields = split /\s+/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 0 ) ? $a_fields[0] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 0 ) ? $b_fields[0] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    $a_num <=> $b_num || $a cmp $b
    } @sort_lines_17_1;
    my $output_17_1 = join "\n", @sort_sorted_17_1;
    if ($output_17_1 ne q{} && !($output_17_1 =~ m{\n\z}msx)) {
    $output_17_1 .= "\n";
    }
    $output_17 = $output_17_1;
    $output_17 = $output_17_1;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', 'final_result.txt'
    or die "Cannot open file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_20 = q{};
    my @lines = split /\n/msx, $output_17;
    my $num_lines = 5;
    if ($num_lines > scalar @lines) {
    $num_lines = scalar @lines;
    }
    my $start_index = scalar @lines - $num_lines;
    if ($start_index < 0) { $start_index = 0; }
    my @result = @lines[$start_index..$#lines];
    $output_17 = join "\n", @result;
    if ($output_17 ne q{} && !($output_17  =~ m{\n\z}msx)) { $output_17 .= "\n"; }
    $tmp_redirect_20;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_17; }
    $output_printed_17 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_17 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
sub __sh2_fnmatch {
    my ($pat, $name) = @_;
    my $re = '';
    for my $i (0 .. length($pat)-1) {
        my $c = substr($pat, $i, 1);
        if ($c eq '*') { $re .= '[^/]*'; }
        elsif ($c eq '?') { $re .= '[^/]'; }
        elsif ($c eq '[') {
            my $j = $i + 1;
            $j++ if substr($pat, $j, 1) eq '!' || substr($pat, $j, 1) eq '^';
            $j++ while $j < length($pat) && substr($pat, $j, 1) ne ']';
            if ($j >= length($pat)) { $re .= '\['; next; }
            my $cls = substr($pat, $i, $j - $i + 1);
            $cls =~ s/^\[!/[^/;
            $re .= $cls; $i = $j;
        }
        else { $re .= "\Q$c\E"; }
    }
    return $name =~ /^${re}$/;
}
sub __sh2_walk {
    my ($dir, $cb, $type, $max, $name, $depth) = @_;
    my $ok0 = ($type eq '' || ($type eq 'f' ? (-f $dir && !-d _) : -d $dir))
        && ($name eq '' || __sh2_fnmatch($name, $dir));
    $cb->($dir) if $ok0;
    opendir(my $dh, $dir) or return;
    while (my $e = readdir($dh)) {
        next if $e eq '.' || $e eq '..';
        my $full = "$dir/$e";
        my $isdir = -d $full;
        my $nm = (split('/', $full))[-1];
        my $ok = ($type eq '' ? 1 : ($type eq 'f' ? (-f $full) : $isdir))
            && ($name eq '' || __sh2_fnmatch($name, $nm));
        $cb->($full) if $ok;
        if ($isdir && ($max == 0 || $depth + 1 < $max)) {
            __sh2_walk($full, $cb, $type, $max, $name, $depth + 1);
        }
    }
}

my $__l2;
my $__l4;
my $__lc2;
my $__lc4;
my $a;
my $b;
my $c;
my $data_type;
my $dir_count;
my $file_count;
my $file_info;
my $host;
my $i;
my $j;
my $letter;
my $letters;
my $num;
my $numbers;
my $port;
my $result;
my $test_string;
my $user;
my $value;
my @letters;
my @matrix;
my @numbers;
my %matrix;

say join(' ', "=== Advanced Bash Idioms Examples ===");
say "";
say join(' ', "1. Nested loops with conditional logic and array manipulation:");
(@numbers = ("1", "2", "3", "4", "5"));
(@letters = ("a", "b", "c", "d", "e"));
for my $__loop_num (@numbers) {
    $num = $__loop_num;
    for my $__loop_letter (@letters) {
        $letter = $__loop_letter;
        if ((($num > 3) && (($letter) != ("c")))) {
            say join(' ', "  Number " . $num . " with letter " . $letter . " (filtered)");
        }
    }
}
say "";
say join(' ', "2. Function with nested case statements and parameter expansion:");
sub process_data {
    # TODO(unsupported): local
    # TODO(unsupported): local
    if (($data_type) =~ m{^(?:"string")$}) {
        if ((lc($value)) =~ m{^(?:"hello"|"hi")$}) {
            say join(' ', "  Greeting detected: " . $value);
        } elsif ((lc($value)) =~ m{^(?:"bye"|"goodbye")$}) {
            say join(' ', "  Farewell detected: " . $value);
        } elsif ((lc($value)) =~ m{^(?:.*)$}) {
            say join(' ', "  Unknown string: " . $value);
        }
    } elsif (($data_type) =~ m{^(?:"number")$}) {
        if ((($value) eq ("~^[0-9]+\$"))) {
            if ((($value % 2) == 0)) {
                say join(' ', "  Even number: " . $value);
            } else {
                say join(' ', "  Odd number: " . $value);
            }
        } else {
            say join(' ', "  Invalid number: " . $value);
        }
    } elsif (($data_type) =~ m{^(?:.*)$}) {
        say join(' ', "  Unknown data type: " . $data_type);
    }
}
process_data("string", "Hello");
process_data("string", "Bye");
process_data("number", "42");
process_data("number", "17");
say "";
say join(' ', "3. Complex conditional with command substitution and arithmetic:");
{
    $__lc2 = 0;
    __sh2_walk(".", sub { $__l2 = shift;
        $__lc2 += 1;
    }, "f", 1, '', 0);
    $file_count = $__lc2;
};
{
    $__lc4 = 0;
    __sh2_walk(".", sub { $__l4 = shift;
        $__lc4 += 1;
    }, "d", 1, '', 0);
    $dir_count = $__lc4;
};
if ((($file_count > 0) && ($dir_count > 1))) {
    if (($file_count > $dir_count)) {
        say join(' ', "  More files (" . $file_count . ") than directories (" . $dir_count . ")");
    } else {
        if (($file_count == $dir_count)) {
            say join(' ', "  Equal count: " . $file_count . " files and " . $dir_count . " directories");
        } else {
            say join(' ', "  More directories (" . $dir_count . ") than files (" . $file_count . ")");
        }
    }
} else {
    say join(' ', "  Insufficient items for comparison");
}
say "";
say join(' ', "4. Nested here-documents with parameter expansion:");
$user = "admin";
$host = "localhost";
$port = "22";
{
        open(my $__sv0, "<&", \*STDIN) or die;
        my $__hs = "    SSH Configuration:\n    \$(cat <<'INNER'\n        User: \$user\n        Host: \$host\n        Port: \$port\n        Status: \$(ping -c 1 \$host >/dev/null 2>&1 && echo \"Online\" || echo \"Offline\")\nINNER\n    )\n";
        open(STDIN, "<", \$__hs) or die;
    system("cat");
        open(STDIN, "<&", $__sv0);
}
say "";
say join(' ', "5. Array processing with nested loops and conditional logic:");
$matrix{"0,0"} = "1";
$matrix{"0,1"} = "2";
$matrix{"0,2"} = "3";
$matrix{"1,0"} = "4";
$matrix{"1,1"} = "5";
$matrix{"1,2"} = "6";
$matrix{"2,0"} = "7";
$matrix{"2,1"} = "8";
$matrix{"2,2"} = "9";
for my $__loop_i (("0", "1", "2")) {
    $i = $__loop_i;
    for my $__loop_j (("0", "1", "2")) {
        $j = $__loop_j;
        $value = $matrix["\$i,\$j"];
        if (($value > 5)) {
            print join(' ', "  [" . $value . "] ");
        } else {
            print join(' ', "  " . $value . " ");
        }
    }
    say "";
}
say "";
say join(' ', "6. Process substitution with nested commands and error handling:");
{
    say join(' ', "  First word: " . do { my $__t = $test_string; $__t =~ s{ .*?$}//; $__t });
    say join(' ', "  Last word: " . do { my $__t = $test_string; $__t =~ s{^.*? }//; $__t });
    say join(' ', "  Middle: " . do { my $__t = $test_string; $__t =~ s{^.* }//; $__t });
    say join(' ', "  Middle: " . do { my $__t = $test_string; $__t =~ s{ .*$}//; $__t });
    say join(' ', "  Uppercase: " . uc($test_string));
    say join(' ', "  Lowercase: " . lc($test_string));
    say join(' ', "  Capitalize: " . ucfirst($test_string));
    say "";
    say join(' ', "11. Complex arithmetic with nested expressions:");
    $a = "10";
    $b = "5";
    $c = "3";
    $result = ((($a + $b) * $c) - int((($a % $b)) / ($c)));
    say join(' ', "  Expression: (a + b) * c - (a % b) / c");
    say join(' ', "  Values: a=" . $a . ", b=" . $b . ", c=" . $c);
    say join(' ', "  Result: " . $result);
    if (((($a > $b) && ($b < $c)) || (($a % 2) == 0))) {
        say join(' ', "  Complex condition met: a > b AND (b < c OR a is even)");
    }
    say "";
    say join(' ', "12. Nested command substitution with error handling:");
    say join(' ', "  Current directory: " . do { my $__qx0 = qx{'pwd'}; $__qx0 =~ s/\n+$//; $__qx0 });
    say join(' ', "  Parent directory: " . do { my $__qx2 = qx{'dirname' \$(do \{ my \$__qx1 = qx\{'pwd'\}; \$__qx1 =~ s/\\n+\$//; \$__qx1 \})}; $__qx2 =~ s/\n+$//; $__qx2 });
    say join(' ', "  Home directory: " . do { my $__qx5 = qx{'dirname' \$(do \{ my \$__qx4 = qx\{'dirname' \\\$(do \\\{ my \\\$__qx3 = qx\\\{'pwd'\\\}; \\\$__qx3 =~ s/\\\\n+\\\$//; \\\$__qx3 \\\})\}; \$__qx4 =~ s/\\n+\$//; \$__qx4 \})}; $__qx5 =~ s/\n+$//; $__qx5 });
    # TODO(unsupported): capture body stmt
    $file_info = do { my $__qx6 = qx{}; $__qx6 =~ s/\n+$//; $__qx6 };
    say join(' ', "  File info: " . $file_info);
    say "";
};
say join(' ', "=== Advanced Bash Idioms Examples Complete ===");
# 3 construct(s) lowered to TODO markers


#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;

print "== Basic grep parameters ==\n";
if (do {
my $output_0 = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern' | grep -i PATTERN}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_0, "\n";
    $CHILD_ERROR == 0
}) {
        print "  -i match: OK\n";
}
if ($CHILD_ERROR != 0) {
        print "  -i match: FAIL\n";
}
my $count = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "line1\\\\nline2\\\\nline3" | grep -v line2 | wc -l}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "  -v count: $count (expected 2)\n";
my $matched = do { open(my $__fh, '-|', 'bash', '-c', q{echo -e "match\\\\nno match\\\\nmatch again" | grep -c match}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "  -c count: $matched (expected 2)\n";
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET" > /tmp/grep_out.txt
do {
    my $output_3 = q{};
    my $output_printed_3;
    my $pipeline_success_3 = 1;
    $output_3 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_3 =~ m{\n\z}) ) { $output_3 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_4 = q{};
    my $grep_result_5;
    my @grep_lines_5 = split /\n/msx, $output_3;
    my @grep_filtered_5 = grep { /TARGET/ } @grep_lines_5;
    my @grep_with_context_5;
    for my $i (0..@grep_lines_5-1) {
    if (scalar grep { $_ eq $grep_lines_5[$i] } @grep_filtered_5) {
    push @grep_with_context_5, $grep_lines_5[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_5, $grep_lines_5[$j];
    }
    }
    }
    $grep_result_5 = join "\n", @grep_with_context_5;
    $CHILD_ERROR = scalar @grep_filtered_5 > 0 ? 0 : 1;
    $tmp_redirect_4 = $grep_result_5;
    $tmp_redirect_4;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_3; }
    $output_printed_3 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_3 ) { $main_exit_code = 1; }
    };
print "  -A 2 lines: " . (do {
    my $wc_file = '/tmp/grep_out.txt';
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
        my $wc_lines = () = $content =~ /\n/gsxm;
        $wc_lines;
    } : q{};
}) . " (expected 3)\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -B 2 "TARGET" > /tmp/grep_out.txt
do {
    my $output_6 = q{};
    my $output_printed_6;
    my $pipeline_success_6 = 1;
    $output_6 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_6 =~ m{\n\z}) ) { $output_6 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_7 = q{};
    my $grep_result_8;
    my @grep_lines_8 = split /\n/msx, $output_6;
    my @grep_filtered_8 = grep { /TARGET/ } @grep_lines_8;
    my @grep_with_context_8;
    for my $i (0..@grep_lines_8-1) {
    if (scalar grep { $_ eq $grep_lines_8[$i] } @grep_filtered_8) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_8, $grep_lines_8[$j];
    }
    }
    push @grep_with_context_8, $grep_lines_8[$i];
    }
    }
    $grep_result_8 = join "\n", @grep_with_context_8;
    $CHILD_ERROR = scalar @grep_filtered_8 > 0 ? 0 : 1;
    $tmp_redirect_7 = $grep_result_8;
    $tmp_redirect_7;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_6; }
    $output_printed_6 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_6 ) { $main_exit_code = 1; }
    };
print "  -B 2 lines: " . (do {
    my $wc_file = '/tmp/grep_out.txt';
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
        my $wc_lines = () = $content =~ /\n/gsxm;
        $wc_lines;
    } : q{};
}) . " (expected 3)\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -C 1 "TARGET" > /tmp/grep_out.txt
do {
    my $output_9 = q{};
    my $output_printed_9;
    my $pipeline_success_9 = 1;
    $output_9 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_9 =~ m{\n\z}) ) { $output_9 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_10 = q{};
    my $grep_result_11;
    my @grep_lines_11 = split /\n/msx, $output_9;
    my @grep_filtered_11 = grep { /TARGET/ } @grep_lines_11;
    my @grep_with_context_11;
    for my $i (0..@grep_lines_11-1) {
    if (scalar grep { $_ eq $grep_lines_11[$i] } @grep_filtered_11) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_11, $grep_lines_11[$j];
    }
    }
    push @grep_with_context_11, $grep_lines_11[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_11, $grep_lines_11[$j];
    }
    }
    }
    $grep_result_11 = join "\n", @grep_with_context_11;
    $CHILD_ERROR = scalar @grep_filtered_11 > 0 ? 0 : 1;
    $tmp_redirect_10 = $grep_result_11;
    $tmp_redirect_10;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_9; }
    $output_printed_9 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_9 ) { $main_exit_code = 1; }
    };
print "  -C 1 lines: " . (do {
    my $wc_file = '/tmp/grep_out.txt';
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
        my $wc_lines = () = $content =~ /\n/gsxm;
        $wc_lines;
    } : q{};
}) . " (expected 3)\n";
print "== File handling parameters ==\n";
open my $fh, '>', '/tmp/grep_file.txt' or die "/tmp/grep_file.txt: $!\n";
print {*fh} "content", "\n";
close $fh;
if (do {
        my $grep_result_12;
    my @grep_lines_12 = ();
    my @grep_filenames_12 = ();
    if (-e "/tmp/grep_file.txt") {
        open my $fh, '<', "/tmp/grep_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_12, $line;
            push @grep_filenames_12, "/tmp/grep_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: /tmp/grep_file.txt: No such file or directory\n"; }
    my @grep_filtered_12 = grep { /content/ } @grep_lines_12;
    $grep_result_12 = scalar @grep_filtered_12 . "\n";
    print $grep_result_12;
    $CHILD_ERROR = scalar @grep_filtered_12 > 0 ? 0 : 1;
    $CHILD_ERROR == 0
}) {
        print "  -c file: OK\n";
}
if ($CHILD_ERROR != 0) {
        print "  -c file: FAIL\n";
}
if (do {
        my $grep_result_13;
    my @grep_lines_13 = ();
    my @grep_filenames_13 = ();
    if (-e "/tmp/grep_file.txt") {
        open my $fh, '<', "/tmp/grep_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_13, $line;
            push @grep_filenames_13, "/tmp/grep_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: /tmp/grep_file.txt: No such file or directory\n"; }
    my @grep_filtered_13 = grep { /content/ } @grep_lines_13;
    $grep_result_13 = @grep_filtered_13 > 0 ? "/tmp/grep_file.txt" : "";
    print $grep_result_13;
    print "\n";
    $CHILD_ERROR = scalar @grep_filtered_13 > 0 ? 0 : 1;
    $CHILD_ERROR == 0
}) {
        print "  -l: found\n";
}
if ($CHILD_ERROR != 0) {
        print "  -l: not found\n";
}
if (do {
        my $grep_result_14;
    my @grep_lines_14 = ();
    my @grep_filenames_14 = ();
    if (-e "/tmp/grep_file.txt") {
        open my $fh, '<', "/tmp/grep_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_14, $line;
            push @grep_filenames_14, "/tmp/grep_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: /tmp/grep_file.txt: No such file or directory\n"; }
    my @grep_filtered_14 = grep { /nonexistent/ } @grep_lines_14;
    $grep_result_14 = @grep_filtered_14 == 0 ? "/tmp/grep_file.txt" : "";
    print $grep_result_14;
    print "\n";
    $CHILD_ERROR = $grep_result_14 ne q{} ? 0 : 1;
    $CHILD_ERROR == 0
}) {
        print "  -L: not found (correct)\n";
}
if ($CHILD_ERROR != 0) {
        print "  -L: found (wrong)\n";
}
print "== Output formatting parameters ==\n";
$matched = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern in it' | grep -o pattern}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "  -o match: '$matched' (expected 'pattern')\n";
my $lineno = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'text with pattern in it' | grep -n pattern | cut -d : -f 1}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "  -n line: $lineno (expected 1)\n";
print "== Recursive parameters ==\n";
if (do {
use File::Path qw(make_path);
my $err;
if ( !-d '/tmp/grep_sub' ) {
    make_path( '/tmp/grep_sub', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . '/tmp/grep_sub' . ": $err->[0]\n";
    }
}
    $CHILD_ERROR == 0
}) {
        open my $fh, '>', '/tmp/grep_sub/file.txt' or die "/tmp/grep_sub/file.txt: $!\n";
    print {*fh} "subfile content", "\n";
    close $fh;
}
my $found = do { open(my $__fh, '-|', 'bash', '-c', q{grep -r subfile /tmp/grep_sub 2> /dev/null | wc -l}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "  -r recursive: $found files matched (expected 1)\n";
if ( -e "/tmp/grep_sub" ) {
    if ( -d "/tmp/grep_sub" ) {
        my $err;
        require File::Path;
        File::Path::remove_tree("/tmp/grep_sub", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "/tmp/grep_sub", ": $err->[0]\n";
        }
        else {
                    }
    }
    else {
        if ( unlink "/tmp/grep_sub" ) {
                    }
        else {
            carp "rm: carping: could not remove ", "/tmp/grep_sub",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}
print "== Line length parameters ==\n";
my $longline = sprintf('a%.0s', "'1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80 81 82 83 84 85 86 87 88 89 90 91 92 93 94 95 96 97 98 99 100 101 102 103 104 105 106 107 108 109 110 111 112 113 114 115 116 117 118 119 120 121 122 123 124 125 126 127 128 129 130 131 132 133 134 135 136 137 138 139 140 141 142 143 144 145 146 147 148 149 150 151 152 153 154 155 156 157 158 159 160 161 162 163 164 165 166 167 168 169 170 171 172 173 174 175 176 177 178 179 180 181 182 183 184 185 186 187 188 189 190 191 192 193 194 195 196 197 198 199 200'");
if (do {
do {
    my $output_19 = q{};
    my $output_printed_19;
    my $pipeline_success_19 = 1;
    $output_19 .= $longline . "\n";
if ( !($output_19 =~ m{\n\z}) ) { $output_19 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_20 = q{};
    my $grep_result_21;
    my @grep_lines_21 = split /\n/msx, $output_19;
    my @grep_filtered_21 = grep { /a/ } @grep_lines_21;
    @grep_filtered_21 = @grep_filtered_21[0..0];
    $grep_result_21 = join "\n", @grep_filtered_21;
    if (!($grep_result_21 =~ m{\n\z} || $grep_result_21 eq q{})) {
    $grep_result_21 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_21 > 0 ? 0 : 1;
    $tmp_redirect_20 = $grep_result_21;
    $tmp_redirect_20;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_19; }
    $output_printed_19 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_19 ) { $main_exit_code = 1; }
    };
    $CHILD_ERROR == 0
}) {
        print "  -m 1 (long line): OK\n";
}
if ($CHILD_ERROR != 0) {
        print "  -m 1 (long line): FAIL\n";
}
print "== Word-regexp and line-regexp parameters ==\n";
# Original bash: echo -e "foo\nfoobar\nbar" | grep -w "foo" > /tmp/grep_out.txt
do {
    my $output_22 = q{};
    my $output_printed_22;
    my $pipeline_success_22 = 1;
    $output_22 .= "foo\nfoobar\nbar";
if ( !($output_22 =~ m{\n\z}) ) { $output_22 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_23 = q{};
    my $grep_result_24;
    my @grep_lines_24 = split /\n/msx, $output_22;
    my @grep_filtered_24 = grep { /foo/ } @grep_lines_24;
    $grep_result_24 = join "\n", @grep_filtered_24;
    if (!($grep_result_24 =~ m{\n\z} || $grep_result_24 eq q{})) {
    $grep_result_24 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_24 > 0 ? 0 : 1;
    $tmp_redirect_23 = $grep_result_24;
    $tmp_redirect_23;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_22; }
    $output_printed_22 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_22 ) { $main_exit_code = 1; }
    };
print "  -w word match lines: " . (do {
    my $wc_file = '/tmp/grep_out.txt';
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
        my $wc_lines = () = $content =~ /\n/gsxm;
        $wc_lines;
    } : q{};
}) . " (expected 1)\n";
unlink('/tmp/grep_file.txt');
unlink('/tmp/grep_out.txt');

exit $main_exit_code;


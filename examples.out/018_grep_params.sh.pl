#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '018_grep_params.sh';
print "== Basic grep parameters ==\n";
if (do {
my $output_186 = qx{command echo 'text with pattern' | grep -i PATTERN};
chomp $output_186;
print $output_186, "\n";
    $CHILD_ERROR == 0
}) {
        print "  -i match: OK\n";
}
if ($CHILD_ERROR != 0) {
        print "  -i match: FAIL\n";
}
my $count = do { chomp(my $_r = qx{command echo -e "line1\\nline2\\nline3" | grep -v line2 | wc -l}); $_r; };
print "  -v count: $count (expected 2)\n";
my $matched = do { chomp(my $_r = qx{command echo -e "match\\nno match\\nmatch again" | grep -c match}); $_r; };
print "  -c count: $matched (expected 2)\n";
print "== Context parameters ==\n";
# Original bash: echo -e "line1\nline2\nTARGET\nline4\nline5" | grep -A 2 "TARGET" > /tmp/grep_out.txt
do {
    my $output_189 = q{};
    my $output_printed_189;
    my $pipeline_success_189 = 1;
    $output_189 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_189 =~ m{\n\z}) ) { $output_189 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_190 = q{};
    my $grep_result_191;
    my @grep_lines_191 = split /\n/msx, $output_189;
    my @grep_filtered_191 = grep { {TARGET} } @grep_lines_191;
    my @grep_with_context_191;
    for my $i (0..@grep_lines_191-1) {
    if (scalar grep { $_ eq $grep_lines_191[$i] } @grep_filtered_191) {
    push @grep_with_context_191, $grep_lines_191[$i];
    for my $j (($i + 1)..($i + 2)) {
    push @grep_with_context_191, $grep_lines_191[$j];
    }
    }
    }
    $grep_result_191 = join "\n", @grep_with_context_191;
    $CHILD_ERROR = scalar @grep_filtered_191 > 0 ? 0 : 1;
    $tmp_redirect_190 = $grep_result_191;
    $tmp_redirect_190;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_189; }
    $output_printed_189 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_189 ) { $main_exit_code = 1; }
    }
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
    my $output_192 = q{};
    my $output_printed_192;
    my $pipeline_success_192 = 1;
    $output_192 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_192 =~ m{\n\z}) ) { $output_192 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_193 = q{};
    my $grep_result_194;
    my @grep_lines_194 = split /\n/msx, $output_192;
    my @grep_filtered_194 = grep { {TARGET} } @grep_lines_194;
    my @grep_with_context_194;
    for my $i (0..@grep_lines_194-1) {
    if (scalar grep { $_ eq $grep_lines_194[$i] } @grep_filtered_194) {
    for my $j (($i - 2)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_194, $grep_lines_194[$j];
    }
    }
    push @grep_with_context_194, $grep_lines_194[$i];
    }
    }
    $grep_result_194 = join "\n", @grep_with_context_194;
    $CHILD_ERROR = scalar @grep_filtered_194 > 0 ? 0 : 1;
    $tmp_redirect_193 = $grep_result_194;
    $tmp_redirect_193;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_192; }
    $output_printed_192 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_192 ) { $main_exit_code = 1; }
    }
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
    my $output_195 = q{};
    my $output_printed_195;
    my $pipeline_success_195 = 1;
    $output_195 .= "line1\nline2\nTARGET\nline4\nline5";
if ( !($output_195 =~ m{\n\z}) ) { $output_195 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_196 = q{};
    my $grep_result_197;
    my @grep_lines_197 = split /\n/msx, $output_195;
    my @grep_filtered_197 = grep { {TARGET} } @grep_lines_197;
    my @grep_with_context_197;
    for my $i (0..@grep_lines_197-1) {
    if (scalar grep { $_ eq $grep_lines_197[$i] } @grep_filtered_197) {
    for my $j (($i - 1)..($i-1)) {
    if ($j >= 0) {
    push @grep_with_context_197, $grep_lines_197[$j];
    }
    }
    push @grep_with_context_197, $grep_lines_197[$i];
    for my $j (($i + 1)..($i + 1)) {
    push @grep_with_context_197, $grep_lines_197[$j];
    }
    }
    }
    $grep_result_197 = join "\n", @grep_with_context_197;
    $CHILD_ERROR = scalar @grep_filtered_197 > 0 ? 0 : 1;
    $tmp_redirect_196 = $grep_result_197;
    $tmp_redirect_196;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_195; }
    $output_printed_195 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_195 ) { $main_exit_code = 1; }
    }
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
        my $grep_result_198;
    my @grep_lines_198 = ();
    my @grep_filenames_198 = ();
    if (-e "/tmp/grep_file.txt") {
        open my $fh, '<', "/tmp/grep_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_198, $line;
            push @grep_filenames_198, "/tmp/grep_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: /tmp/grep_file.txt: No such file or directory\n"; }
    my @grep_filtered_198 = grep { {content} } @grep_lines_198;
    $grep_result_198 = scalar @grep_filtered_198 . "\n";
    print $grep_result_198;
    $CHILD_ERROR = scalar @grep_filtered_198 > 0 ? 0 : 1;
    $CHILD_ERROR == 0
}) {
        print "  -c file: OK\n";
}
if ($CHILD_ERROR != 0) {
        print "  -c file: FAIL\n";
}
if (do {
        my $grep_result_199;
    my @grep_lines_199 = ();
    my @grep_filenames_199 = ();
    if (-e "/tmp/grep_file.txt") {
        open my $fh, '<', "/tmp/grep_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_199, $line;
            push @grep_filenames_199, "/tmp/grep_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: /tmp/grep_file.txt: No such file or directory\n"; }
    my @grep_filtered_199 = grep { {content} } @grep_lines_199;
    $grep_result_199 = @grep_filtered_199 > 0 ? "/tmp/grep_file.txt" : "";
    print $grep_result_199;
    print "\n";
    $CHILD_ERROR = scalar @grep_filtered_199 > 0 ? 0 : 1;
    $CHILD_ERROR == 0
}) {
        print "  -l: found\n";
}
if ($CHILD_ERROR != 0) {
        print "  -l: not found\n";
}
if (do {
        my $grep_result_200;
    my @grep_lines_200 = ();
    my @grep_filenames_200 = ();
    if (-e "/tmp/grep_file.txt") {
        open my $fh, '<', "/tmp/grep_file.txt" or croak "Cannot access file: $ERRNO";
        while (my $line = <$fh>) {
            chomp $line;
            push @grep_lines_200, $line;
            push @grep_filenames_200, "/tmp/grep_file.txt";
        }
        close $fh
            or croak "Close failed: $OS_ERROR";
    }
    else { print {*STDERR} "grep: /tmp/grep_file.txt: No such file or directory\n"; }
    my @grep_filtered_200 = grep { {nonexistent} } @grep_lines_200;
    $grep_result_200 = @grep_filtered_200 == 0 ? "/tmp/grep_file.txt" : "";
    print $grep_result_200;
    print "\n";
    $CHILD_ERROR = $grep_result_200 ne q{} ? 0 : 1;
    $CHILD_ERROR == 0
}) {
        print "  -L: not found (correct)\n";
}
if ($CHILD_ERROR != 0) {
        print "  -L: found (wrong)\n";
}
print "== Output formatting parameters ==\n";
$matched = do { chomp(my $_r = qx{command echo 'text with pattern in it' | grep -o pattern}); $_r; };
print "  -o match: '$matched' (expected 'pattern')\n";
my $lineno = do { chomp(my $_r = qx{command echo 'text with pattern in it' | grep -n pattern | cut -d : -f 1}); $_r; };
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
my $found = do { chomp(my $_r = qx{command grep -r subfile /tmp/grep_sub 2> /dev/null | wc -l}); $_r; };
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
    my $output_205 = q{};
    my $output_printed_205;
    my $pipeline_success_205 = 1;
    $output_205 .= $longline . "\n";
if ( !($output_205 =~ m{\n\z}) ) { $output_205 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_206 = q{};
    my $grep_result_207;
    my @grep_lines_207 = split /\n/msx, $output_205;
    my @grep_filtered_207 = grep { {a} } @grep_lines_207;
    @grep_filtered_207 = @grep_filtered_207[0..0];
    $grep_result_207 = join "\n", @grep_filtered_207;
    if (!($grep_result_207 =~ m{\n\z} || $grep_result_207 eq q{})) {
    $grep_result_207 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_207 > 0 ? 0 : 1;
    $tmp_redirect_206 = $grep_result_207;
    $tmp_redirect_206;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_205; }
    $output_printed_205 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_205 ) { $main_exit_code = 1; }
    }
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
    my $output_208 = q{};
    my $output_printed_208;
    my $pipeline_success_208 = 1;
    $output_208 .= "foo\nfoobar\nbar";
if ( !($output_208 =~ m{\n\z}) ) { $output_208 .= "\n"; }

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/tmp/grep_out.txt'
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_209 = q{};
    my $grep_result_210;
    my @grep_lines_210 = split /\n/msx, $output_208;
    my @grep_filtered_210 = grep { {foo} } @grep_lines_210;
    $grep_result_210 = join "\n", @grep_filtered_210;
    if (!($grep_result_210 =~ m{\n\z} || $grep_result_210 eq q{})) {
    $grep_result_210 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_210 > 0 ? 0 : 1;
    $tmp_redirect_209 = $grep_result_210;
    $tmp_redirect_209;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_208; }
    $output_printed_208 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_208 ) { $main_exit_code = 1; }
    }
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

#!/usr/bin/perl
BEGIN { $0 = "examples.impurl/044_yes_command.pl" }


print "=== Example 044: yes command ===\n";

print "Using backticks to call yes (limited output):\n";
my $yes_output = do { my $pipeline_cmd = q{yes 'Hello World' | head -5}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_output;

print "\nyes with specific string:\n";
do {
    my $__PURIFY_TMP = do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $head_line_count = 0;
my $output_0 = q{};
while (1) {
    my $line = 'Test String';
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_0 .= "\n"; }
    $output_0 .= $line;
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0

    };
    if (defined $__PURIFY_TMP && $__PURIFY_TMP ne q{}) {
        print $__PURIFY_TMP;
        if (!($__PURIFY_TMP =~ m{\n\z}msx)) { print "\n"; }
    }
};

print "\nyes with default string:\n";
my $yes_default = do { my $pipeline_cmd = 'yes | head -3'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_default;

print "\nyes with empty string:\n";
do {
    my $__PURIFY_TMP = do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $head_line_count = 0;
my $output_0 = q{};
while (1) {
    my $line = q{};
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_0 .= "\n"; }
    $output_0 .= $line;
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0

    };
    if (defined $__PURIFY_TMP && $__PURIFY_TMP ne q{}) {
        print $__PURIFY_TMP;
        if (!($__PURIFY_TMP =~ m{\n\z}msx)) { print "\n"; }
    }
};

print "\nyes with special characters:\n";
my $yes_special = do { my $pipeline_cmd = q{yes '!@#$%^&*()' | head -3}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_special;

print "\nyes with numbers:\n";
do {
    my $__PURIFY_TMP = do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $head_line_count = 0;
my $output_0 = q{};
while (1) {
    my $line = '12345';
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_0 .= "\n"; }
    $output_0 .= $line;
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0

    };
    if (defined $__PURIFY_TMP && $__PURIFY_TMP ne q{}) {
        print $__PURIFY_TMP;
        if (!($__PURIFY_TMP =~ m{\n\z}msx)) { print "\n"; }
    }
};

print "\nyes with newlines:\n";
my $yes_newlines = do { my $pipeline_cmd = q{yes 'Line with\nnewline' | head -3}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_newlines;

print "\nyes with pipe to other commands:\n";
do {
    my $__PURIFY_TMP = do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $head_line_count = 0;
my $output_0 = q{};
while (1) {
    my $line = 'test';
    # yes doesn't support line-by-line processing
    if (!($line =~ /test/msx)) {
    next;
    }
    if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_0 .= "\n"; }
    $output_0 .= $line;
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0

    };
    if (defined $__PURIFY_TMP && $__PURIFY_TMP ne q{}) {
        print $__PURIFY_TMP;
        if (!($__PURIFY_TMP =~ m{\n\z}msx)) { print "\n"; }
    }
};

print "\nyes with pipe to other commands:\n";
my $yes_pipe = do { my $pipeline_cmd = 'yes data | tr a-z A-Z | head -3'; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_pipe;

print "\nyes with output redirection:\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        my $string = 'Output to file';
    $output_0 = q{};
    for (my $i = 0; $i < 1000; $i++) {
    $output_0 .= "$string\n";
    }
    $output_0;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $!\n";
    open STDOUT, '>', 'yes_output.txt'
    or die "Cannot open file: $!\n";
    my $tmp = do {
    my $tmp_redirect_1 = q{};
    my $num_lines       = 5;
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
    $tmp_redirect_1;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_0; }
    $output_printed_0 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $!\n";
    close $original_stdout
    or die "Close failed: $!\n";
    };
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

if (-f "yes_output.txt") {
    print "Output file created successfully\n";
    my $file_content = do { open my $fh, '<', 'yes_output.txt' or die 'cat: ' . 'yes_output.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "File content:\n$file_content";
}

print "\nyes with background process (bounded):\n";
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);if (my $pid = fork()) {
    # Parent process continues
} elsif (defined $pid) {
    # Child process executes the background command
    # Original bash: yes 'Background' | head -n 3 > /dev/null &
{
        my $output_0 = q{};
        my $output_printed_0;
        my $pipeline_success_0 = 1;
                my $string = 'Background';
        $output_0 = q{};
        for (my $i = 0; $i < 1000; $i++) {
        $output_0 .= "$string\n";
        }
        $output_0;

                do {
        open my $original_stdout, '>&', STDOUT
        or die "Cannot save STDOUT: $!\n";
        open STDOUT, '>', '/dev/null'
        or die "Cannot open file: $!\n";
        my $tmp = do {
        my $tmp_redirect_1 = q{};
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
        $tmp_redirect_1;
        };
        print $tmp;
        if ($tmp eq q{}) { print $output_0; }
        $output_printed_0 = 1;
        open STDOUT, '>&', $original_stdout
        or die "Cannot restore STDOUT: $!\n";
        close $original_stdout
        or die "Close failed: $!\n";
        };
        if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
        }
    exit(0);
} else {
    die "Cannot fork: $!\n";
}
print "Background process started (will exit shortly)\n";

print "\nyes with timeout (bounded):\n";
do {
    my $__PURIFY_TMP = do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $head_line_count = 0;
my $output_0 = q{};
while (1) {
    my $line = 'Timeout test';
    # yes doesn't support line-by-line processing
    if ($head_line_count < 3) {
    if ($head_line_count > 0) { $output_0 .= "\n"; }
    $output_0 .= $line;
    ++$head_line_count;
    } else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
    }
}
$output_0

    };
    if (defined $__PURIFY_TMP && $__PURIFY_TMP ne q{}) {
        print $__PURIFY_TMP;
        if (!($__PURIFY_TMP =~ m{\n\z}msx)) { print "\n"; }
    }
};

print "\nyes with different strings:\n";
my $yes_diff = do { my $pipeline_cmd = q{yes 'String 1' | head -2}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_diff;
my $yes_diff2 = do { my $pipeline_cmd = q{yes 'String 2' | head -2}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print $yes_diff2;

print "\nyes with error handling:\n";
do {
    my $__PURIFY_TMP = do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
{
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    my $string = 'Error test';
$output_0 = q{};
for (my $i = 0; $i < 1000; $i++) {
    $output_0 .= "$string\n";
}
$output_0;

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
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

    };
    if (defined $__PURIFY_TMP && $__PURIFY_TMP ne q{}) {
        print $__PURIFY_TMP;
        if (!($__PURIFY_TMP =~ m{\n\z}msx)) { print "\n"; }
    }
};

print "\nyes with pipe to wc:\n";
my $yes_wc = do { my $pipeline_cmd = q{yes 'Count me' | head -10 | wc -l}; my $result = qx{$pipeline_cmd}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Count: $yes_wc";

unlink('yes_output.txt') if -f 'yes_output.txt';

print "=== Example 044 completed successfully ===\n";

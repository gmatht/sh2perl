#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/030_tee_basic.pl" }


print "=== Example 030: Basic tee command ===\n";

print "Using backticks to call tee (write to file and stdout):\n";
my $tee_output = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'This is a test line' . "\n";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee_output.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee_output.txt': $!";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Output: $tee_output";

if (-f "test_tee_output.txt") {
    print "File created successfully\n";
    my $file_content = do { open my $fh, '<', 'test_tee_output.txt' or die 'cat: ' . 'test_tee_output.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; }
;
    print "File content: $file_content";
}

print "\ntee with append (-a):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('echo', 'This is another line', '|', 'tee', '-a', 'test_tee_output.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntee with multiple files:\n";
my $tee_multi = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'Line for multiple files' . "\n";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee1.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee1.txt': $!";
    }
    if ( open my $fh, '>', 'test_tee2.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee2.txt': $!";
    }
    if ( open my $fh, '>', 'test_tee3.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee3.txt': $!";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Output: $tee_multi";

if (-f "test_tee1.txt" && -f "test_tee2.txt" && -f "test_tee3.txt") {
    print "Multiple files created successfully\n";
}

print "\ntee with ignore interrupts (-i):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('echo', 'This line ignores interrupts', '|', 'tee', '-i', 'test_tee_interrupt.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntee with pipe fail (-p):\n";
my $tee_pipe = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'This line has pipe fail' . "\n";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee_pipe.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee_pipe.txt': $!";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Output: $tee_pipe";

print "\ntee with append and multiple files:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('echo', 'Appended line', '|', 'tee', '-a', 'test_tee1.txt', 'test_tee2.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntee with output to stderr:\n";
my $tee_stderr = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'This goes to stderr' . "\n";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    use Carp qw(carp croak);
    use IO::Handle;
    STDOUT->flush();
    if ( open my $fh, '>', '/dev/stderr' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open /dev/stderr: $!";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Output: $tee_stderr";

print "\ntee with null output:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('echo', 'This goes to null', '|', 'tee', '/dev/null'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntee with multiple outputs:\n";
my $tee_multi_out = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'Multiple outputs' . "\n";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee_multi1.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee_multi1.txt': $!";
    }
    if ( open my $fh, '>', 'test_tee_multi2.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee_multi2.txt': $!";
    }
    $output_0 = $output_0 . $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Output: $tee_multi_out";

print "\ntee with append and ignore interrupts:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ('echo', 'Appended with ignore interrupts', '|', 'tee', '-a', '-i', 'test_tee_append_interrupt.txt'); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\ntee with pipe fail and multiple files:\n";
my $tee_pipe_multi = do { do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
    $output_0 .= 'Pipe fail with multiple files' . "\n";
    if ( !($output_0 =~ m{\n\z}msx) ) { $output_0 .= "\n"; }
    $CHILD_ERROR = 0;
    use Carp qw(carp croak);
    if ( open my $fh, '>', 'test_tee_pipe1.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee_pipe1.txt': $!";
    }
    if ( open my $fh, '>', 'test_tee_pipe2.txt' ) {
        print {$fh} $output_0;
        close $fh or croak "Close failed: $!";
    }
    else {
        carp "tee: Cannot open 'test_tee_pipe2.txt': $!";
    }
    $output_0 = $output_0;
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) {
        $output_0 .= "\n";
    }
    $output_0;
} }
;
print "Output: $tee_pipe_multi";

unlink('test_tee_output.txt') if -f 'test_tee_output.txt';
unlink('test_tee1.txt') if -f 'test_tee1.txt';
unlink('test_tee2.txt') if -f 'test_tee2.txt';
unlink('test_tee3.txt') if -f 'test_tee3.txt';
unlink('test_tee_interrupt.txt') if -f 'test_tee_interrupt.txt';
unlink('test_tee_pipe.txt') if -f 'test_tee_pipe.txt';
unlink('test_tee_multi1.txt') if -f 'test_tee_multi1.txt';
unlink('test_tee_multi2.txt') if -f 'test_tee_multi2.txt';
unlink('test_tee_append_interrupt.txt') if -f 'test_tee_append_interrupt.txt';
unlink('test_tee_pipe1.txt') if -f 'test_tee_pipe1.txt';
unlink('test_tee_pipe2.txt') if -f 'test_tee_pipe2.txt';

print "=== Example 030 completed successfully ===\n";

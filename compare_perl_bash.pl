#!/usr/bin/env perl
use strict;
use warnings;

# Get the command from command line arguments
my $command = shift @ARGV;
if (!$command) {
    die "Usage: $0 <command>\nExample: $0 'echo hi'\n";
}

print "Comparing Perl vs Bash execution for: $command\n";
print "=" x 60 . "\n\n";

# Create temporary files for output
my ($perl_fh, $perl_file) = tempfile(SUFFIX => '.pl', UNLINK => 1);
my ($bash_fh, $bash_file) = tempfile(SUFFIX => '.sh', UNLINK => 1);

# Run debashc.exe to get the Perl code
print "1. Generating Perl code from debashc.exe...\n";
my $start_time = [gettimeofday];
my ($perl_code, $perl_stderr, $perl_exit) = run_debashc($command);
my $perl_gen_time = tv_interval($start_time);

if ($perl_exit != 0) {
    die "Error generating Perl code: $perl_stderr\n";
}

# Extract just the Perl code (remove the wrapper output)
my $clean_perl_code = extract_perl_code($perl_code);
print $perl_fh $clean_perl_code;
close $perl_fh;

# Create bash script
print $bash_fh "#!/bin/bash\n$command\n";
close $bash_fh;

print "2. Running Perl version...\n";
my $perl_start = [gettimeofday];
my ($perl_output, $perl_err, $perl_exit_code) = run_perl($perl_file);
my $perl_run_time = tv_interval($perl_start);

print "3. Running Bash version...\n";
my $bash_start = [gettimeofday];
my ($bash_output, $bash_err, $bash_exit_code) = run_bash($bash_file);
my $bash_run_time = tv_interval($bash_start);

# Display results
print "\n" . "=" x 60 . "\n";
print "RESULTS COMPARISON\n";
print "=" x 60 . "\n\n";

print "PERL OUTPUT:\n";
print "-" x 20 . "\n";
print $perl_output;
print "Exit code: $perl_exit_code\n\n";

print "BASH OUTPUT:\n";
print "-" x 20 . "\n";
print $bash_output;
print "Exit code: $bash_exit_code\n\n";

# Timing information
print "TIMING INFORMATION:\n";
print "-" x 20 . "\n";
printf "Perl generation time: %.4f seconds\n", $perl_gen_time;
printf "Perl execution time:  %.4f seconds\n", $perl_run_time;
printf "Bash execution time:  %.4f seconds\n", $bash_run_time;
printf "Total Perl time:      %.4f seconds\n", $perl_gen_time + $perl_run_time;
print "\n";

# Speedup calculation
my $total_perl_time = $perl_gen_time + $perl_run_time;
my $speedup = $total_perl_time > 0 ? $bash_run_time / $total_perl_time : 0;
my $perl_overhead = $total_perl_time - $bash_run_time;

printf "SPEEDUP ANALYSIS:\n";
printf "-" x 20 . "\n";
if ($speedup > 1) {
    printf "Bash is %.2fx faster than Perl (including generation)\n", $speedup;
} else {
    printf "Perl is %.2fx faster than Bash (including generation)\n", 1/$speedup;
}
printf "Perl overhead: %.4f seconds (%.1f%% of total time)\n", 
    $perl_overhead, ($perl_overhead / $total_perl_time) * 100;

# Diff output
print "\nDIFF OUTPUT:\n";
print "-" x 20 . "\n";
if ($perl_output eq $bash_output && $perl_exit_code == $bash_exit_code) {
    print "✓ PERFECT MATCH: Perl and Bash outputs are identical!\n";
} else {
    print "✗ DIFFERENCES FOUND:\n";
    
    if ($perl_output ne $bash_output) {
        print "\nOutput differences:\n";
        my $diff = generate_diff($bash_output, $perl_output, "Bash", "Perl");
        print $diff;
    }
    
    if ($perl_exit_code != $bash_exit_code) {
        print "\nExit code differences:\n";
        print "  Bash exit code: $bash_exit_code\n";
        print "  Perl exit code: $perl_exit_code\n";
    }
}

# Cleanup
unlink $perl_file, $bash_file;

sub run_debashc {
    my ($cmd) = @_;
    my ($stdout, $stderr);
    my $exit_code = run(['target/debug/debashc.exe', $cmd], \$stdout, \$stderr);
    return ($stdout, $stderr, $exit_code);
}

sub extract_perl_code {
    my ($output) = @_;
    # Extract the Perl code between the markers
    if ($output =~ /Generated Perl code:\n(.*?)\n--- Running generated Perl code ---/s) {
        return $1;
    }
    return $output;
}

sub run_perl {
    my ($file) = @_;
    my ($stdout, $stderr);
    my $exit_code = run(['perl', $file], \$stdout, \$stderr);
    return ($stdout, $stderr, $exit_code);
}

sub run_bash {
    my ($file) = @_;
    my ($stdout, $stderr);
    my $exit_code = run(['bash', $file], \$stdout, \$stderr);
    return ($stdout, $stderr, $exit_code);
}

sub generate_diff {
    my ($bash_out, $perl_out, $bash_label, $perl_label) = @_;
    
    # Simple diff implementation
    my @bash_lines = split /\n/, $bash_out;
    my @perl_lines = split /\n/, $perl_out;
    
    my $diff = "";
    my $max_lines = @bash_lines > @perl_lines ? @bash_lines : @perl_lines;
    
    for my $i (0..$max_lines-1) {
        my $bash_line = $bash_lines[$i] // "";
        my $perl_line = $perl_lines[$i] // "";
        
        if ($bash_line ne $perl_line) {
            $diff .= sprintf "%3d: %s | %s\n", $i+1, 
                $bash_line eq "" ? "(empty)" : $bash_line,
                $perl_line eq "" ? "(empty)" : $perl_line;
        }
    }
    
    return $diff || "No line-by-line differences found\n";
}

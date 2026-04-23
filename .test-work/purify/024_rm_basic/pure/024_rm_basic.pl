use Carp;
#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/024_rm_basic.pl" }


print "=== Example 024: Basic rm command ===\n";

open(my $fh, '>', 'test_rm_file1.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for removal\n";
close($fh);

open(my $fh2, '>', 'test_rm_file2.txt') or die "Cannot create test file: $!\n";
print $fh2 "This is another test file\n";
close($fh2);

my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mkdir", "-p", "test_rm_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_rm_dir/file3.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "Using " . "sys" . "tem" . "() to call rm (remove file):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "test_rm_file1.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
if (!-f "test_rm_file1.txt") {
    print "File removed successfully\n";
} else {
    print "File removal failed\n";
}

print "\nrm with verbose (-v):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-v", "test_rm_file2.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nrm with force (-f):\n";
my $rm_force = do { my $command = 'rm -f test_rm_file2.txt 2> /dev/null'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
print "Force removal attempted\n";


print "\nrm with recursive (-r):\n";
my $rm_recursive = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            if ( -e "test_rm_dir" ) {
                if ( -d "test_rm_dir" ) {
                    my $err;
                    require File::Path;
                    File::Path::remove_tree("test_rm_dir", {error => \$err});
                    if (@{$err}) {
                        croak "rm: cannot remove ", "test_rm_dir", ": $err->[0]\n";
                    }
                    else {
                        $main_exit_code = 0;
                    }
                }
                else {
                    if ( unlink "test_rm_dir" ) {
                        $main_exit_code = 0;
                    }
                    else {
                        croak "rm: cannot remove ", "test_rm_dir",
                          ": $!\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: ", "test_rm_dir", ": No such file or directory\n";
            }
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (!-d "test_rm_dir") {
    print "Directory removed recursively\n";
}

print "\nrm with recursive and force (-rf):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mkdir", "-p", "test_rm_dir2/subdir"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_rm_dir2/file.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_rm_dir2/subdir/file2.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_rm_dir2"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nrm with preserve root (--preserve-root):\n";
my $rm_preserve = do { my $command = q{rm -rf / 2> /dev/null || echo 'Protected from removing root'}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
print $rm_preserve;

print "\nrm with one file " . "sys" . "tem" . " (-x):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mkdir", "-p", "test_rm_xfs"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-x", "test_rm_xfs"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "\nrm with no dereference (-P):\n";
my $rm_no_deref = do { my $command = q{rm -P test_rm_xfs 2> /dev/null || echo 'No files to remove'}; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
print $rm_no_deref;

print "\nrm with ignore missing (-f):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-f", "nonexistent_file.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print "Ignored missing file\n";

print "\nrm with directory (-d):\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("mkdir", "-p", "test_rm_empty_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $rm_dir = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do { my $rm_cmd = 'rm -d test_rm_empty_dir'; qx{$rm_cmd}; };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (!-d "test_rm_empty_dir") {
    print "Empty directory removed\n";
}

print "\nrm with multiple files:\n";
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_rm_multi1.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_rm_multi2.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("touch", "test_rm_multi3.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "test_rm_multi1.txt", "test_rm_multi2.txt", "test_rm_multi3.txt"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
print "Multiple files removed\n";

unlink('test_rm_file1.txt') if -f 'test_rm_file1.txt';
unlink('test_rm_file2.txt') if -f 'test_rm_file2.txt';
unlink('test_rm_interactive.txt') if -f 'test_rm_interactive.txt';
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_rm_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_rm_dir2"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_rm_xfs"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec ("rm", "-rf", "test_rm_empty_dir"); die "exec failed: " . $!; } else { waitpid($pid, 0); }$?;

print "=== Example 024 completed successfully ===\n";

#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $XAR = 'ar';

sub mcarfs_list {
    my ($file) = @_;
    my $temp_replace = 'Unique Separator String';
    my $thisyear = (do {
require POSIX; POSIX::strftime('%Y', localtime())
});
    # Original bash: $XAR tv "$_[0]" | sed 's,^,-,;s, , 1 ,;s,/, ,' |
do {
        my $output_0 = q{};
        my $output_printed_0;
        my $pipeline_success_0 = 1;
                my ($in_1, $out_1);
        my $pid_1 = open3($in_1, $out_1, '>&STDERR', 'unknown_command', 'tv');
        close $in_1 or croak 'Close failed: $OS_ERROR';
        $output_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_1> };
        close $out_1 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_1, 0;

                my @sed_lines_0 = split /\n/, $output_0;
        my @sed_result_0;
        foreach my $line (@sed_lines_0) {
        chomp $line;
        push @sed_result_0, $line;
        }
        $output_0 = join "\n", @sed_result_0;

                my @sed_lines_0 = split /\n/, $output_0;
        my @sed_result_0;
        foreach my $line (@sed_lines_0) {
        chomp $line;
        push @sed_result_0, $line;
        }
        $output_0 = join "\n", @sed_result_0;

                my @sed_lines_0 = split /\n/, $output_0;
        my @sed_result_0;
        foreach my $line (@sed_lines_0) {
        chomp $line;
        push @sed_result_0, $line;
        }
        $output_0 = join "\n", @sed_result_0;

                my @sed_lines_0 = split /\n/, $output_0;
        my @sed_result_0;
        foreach my $line (@sed_lines_0) {
        chomp $line;
        push @sed_result_0, $line;
        }
        $output_0 = join "\n", @sed_result_0;
        if ($output_0 ne q{} && !defined $output_printed_0) {
            print $output_0;
            if (!($output_0 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
        }
;
    return;
}

sub mcarfs_copyout {
    my ($file) = @_;
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$_[2]"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $CHILD_ERROR = 0;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub mcarfs_copyin {
    my ($file) = @_;
        my $TMPDIR = do {
    my ($in_2, $out_2);
    my $pid_2 = open3($in_2, $out_2, '>&STDERR', 'mktemp', '-d', (defined (defined ($ENV{MC_TMPDIR} // q{}) && ($ENV{MC_TMPDIR} // q{}) ne q{} ? ($ENV{MC_TMPDIR} // q{}) : '/tmp') && (defined ($ENV{MC_TMPDIR} // q{}) && ($ENV{MC_TMPDIR} // q{}) ne q{} ? ($ENV{MC_TMPDIR} // q{}) : '/tmp') ne q{} ? (defined ($ENV{MC_TMPDIR} // q{}) && ($ENV{MC_TMPDIR} // q{}) ne q{} ? ($ENV{MC_TMPDIR} // q{}) : '/tmp') : '/tmp') . "/mctmpdir-uar.XXXXXX");
    close $in_2 or croak 'Close failed: $OS_ERROR';
    my $result_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_2> };
    close $out_2 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_2, 0;
    $result_2
};
    if ($CHILD_ERROR != 0) {
        exit 1;
    }
;
    my $name = do { use File::Basename qw(basename); my $basename_output = basename("$_[1]"); $CHILD_ERROR = 0; $basename_output; };
    do {
        local %ENV = %ENV;
        my $TMPDIR = $TMPDIR;
        my $name = $name;
        my $XAR = $XAR;
        if (do {
if (do {
chdir("$TMPDIR");
$CHILD_ERROR = 0;
    $CHILD_ERROR == 0
}) {
        use File::Copy qw(copy);
    if ( -e q{p} ) {
        if ( -d "$name" ) {
            require File::Copy; File::Copy::copy(q{p}, "$name" . '/' . (q{p} =~ m|([^/]+)$|)[0]);
        } else {
            require File::Copy; File::Copy::copy(q{p}, "$name");
        }
    } else {
        croak "cp: cannot stat '-f': No such file or directory\n";
    }
    if ( -e "$_[2]" ) {
        if ( -d "$name" ) {
            require File::Copy; File::Copy::copy("$_[2]", "$name" . '/' . ("$_[2]" =~ m|([^/]+)$|)[0]);
        } else {
            require File::Copy; File::Copy::copy("$_[2]", "$name");
        }
    } else {
        croak "cp: cannot stat '-f': No such file or directory\n";
    }
}
            $CHILD_ERROR == 0
        }) {
                        $CHILD_ERROR = 0;
        }
        q{};
    };
if ( -e "$TMPDIR" ) {
        if ( -d "$TMPDIR" ) {
            my $err;
            require File::Path;
            File::Path::remove_tree("$TMPDIR", {error => \$err});
            if (@{$err}) {
                carp "rm: carping: could not remove ", "$TMPDIR", ": $err->[0]\n";
            }
            else {
                            }
        }
        else {
            if ( unlink "$TMPDIR" ) {
                            }
            else {
                carp "rm: carping: could not remove ", "$TMPDIR",
              ": $OS_ERROR\n";
            }
        }
    }
    else {
        local $CHILD_ERROR = 0;
    }
    return;
}

sub mcarfs_rm {
    my ($file) = @_;
    $CHILD_ERROR = 0;
    return;
}
my $LC_ALL = q{C};
$ENV{LC_ALL} = $LC_ALL;
$main_exit_code = system('umask', q{077}) >> 8;
if ("$_[0]" eq 'list') {
        mcarfs_list("$_[1]");
} elsif ("$_[0]" eq 'copyout') {
    # Builtin command 'shift' not implemented
        mcarfs_copyout("\@ARGV");
} elsif ("$_[0]" eq 'copyin') {
    # Builtin command 'shift' not implemented
        mcarfs_copyin("\@ARGV");
} elsif ("$_[0]" eq 'rm') {
    # Builtin command 'shift' not implemented
        mcarfs_rm("\@ARGV");
} elsif ("$_[0]" eq 'mkdir' or "$_[0]" eq 'rmdir') {
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say "mcarfs: ar archives cannot contain directories.";
    };
    exit 1;
} elsif (1) {
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say "mcarfs: unknown command: \"$_[0]\".";
    };
    exit 1;
}
exit 0;

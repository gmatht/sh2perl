#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $SNDOPTIONS;
my $SYSFS;
my $REPEAT;
my $CONFIRM;
my $DIALOG;
my $ACPI_STATUS;
my $PWINST;
my $path;
my $DIALOG_EXIT_CODE;
my $WELCOME;
my $WITHALL;
my $TEMPDIR;
my $ESDINST;
my $JACKINST;
my $ARTSINST;
my $PROCEED;
my $PASTEBIN;
my $LSPCI;
my $PAINST;
my $JACK2INST;
my $TPUT;
my $UPLOAD;
my $KERNEL_VERSION;
my $DMIDECODE;
my $NFILE;
my $ROARINST;

my $SCRIPT_VERSION = '0.5.3';
my $CHANGELOG = 'https://www.alsa-project.org/alsa-info.sh.changelog';
$ENV{LC_ALL} = 'C';
my $PATH = "$ENV{PATH}:/bin:/sbin:/usr/bin:/usr/sbin";
my $BGTITLE = "ALSA-Info v $SCRIPT_VERSION";
my $PASTEBINKEY = 'C9cRIO8m/9y8Cs0nVs0FraRx7U0pHsuc';
my $WGET = (do {
    my ($in_0, $out_0);
    my $pid_0 = open3($in_0, $out_0, '>&STDERR', 'command', '-v', 'wget');
    close $in_0 or croak 'Close failed: $OS_ERROR';
    my $result_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_0> };
    close $out_0 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_0, 0;
    $result_0
});
my @REQUIRES = ('mktemp', 'grep', 'pgrep', 'awk', 'date', 'uname', 'cat', 'sort', 'dmesg', 'amixer', 'alsactl');

sub update {
    my ($file) = @_;
    if (do {
$main_exit_code = system('test', '-z', "$WGET") >> 8;
if ($CHILD_ERROR != 0) {
        $main_exit_code = system('test', q{!}, '-x', "$WGET") >> 8;
}
        $CHILD_ERROR == 0
    }) {
        return;
    }
        my $SHFILE = do {
    my ($in_1, $out_1);
    my $pid_1 = open3($in_1, $out_1, '>&STDERR', 'mktemp', '-t', 'alsa-info.XXXXXXXXXX');
    close $in_1 or croak 'Close failed: $OS_ERROR';
    my $result_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_1> };
    close $out_1 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_1, 0;
    $result_1
};
    if ($CHILD_ERROR != 0) {
        exit 1;
    }
;
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
use LWP::Simple;
my $url = "https://www.alsa-project.org/alsa-info.sh";
my $output_file = $SHFILE;
my $content = get($url);
if (defined $content) {
open my $fh, '>', $output_file or die "Cannot open $output_file: $ERRNO";
print {$fh} $content;
close $fh or croak "Close failed: $ERRNO";
print "Downloaded to $output_file\n";
} else {
die "Failed to download $url\n";
}
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    my $REMOTE_VERSION = do { my $result_3 = qx{bash -c q{grep SCRIPT_VERSION Variable("SHFILE", false, None) | head -n 1 | sed 's/.*=//'} }; chomp $result_3; $result_3; };
    my $OVERWRITE;
if ((((-s "$SHFILE") > 0) && "$REMOTE_VERSION" ne "$SCRIPT_VERSION")) {
if ($DIALOG ne q{}) {
            $OVERWRITE = q{};
if ((-w $0)) {
                $main_exit_code = system('dialog', '--yesno', "Newer version of ALSA-Info has been found\n\nDo you wish to install it?\nNOTICE: The original file $PROGRAM_NAME will be overwritten!", q{0}, q{0}) >> 8;
                $DIALOG_EXIT_CODE = $?;
if ($DIALOG_EXIT_CODE eq 0) {
                    $OVERWRITE = 'yes';
                }
            }
if ("$OVERWRITE" eq q{}) {
                $main_exit_code = system('dialog', '--yesno', "Newer version of ALSA-Info has been found\n\nDo you wish to download it?", q{0}, q{0}) >> 8;
                $DIALOG_EXIT_CODE = $?;
            }
if ($DIALOG_EXIT_CODE eq 0) {
                say "Newer version detected: $REMOTE_VERSION";
                say "To view the ChangeLog, please visit $CHANGELOG";
if ("$OVERWRITE" eq "yes") {
                    use File::Copy qw(copy);
                    if ( -e $SHFILE ) {
                        if ( -d $PROGRAM_NAME ) {
                            require File::Copy; File::Copy::copy($SHFILE, $PROGRAM_NAME . '/' . ($SHFILE =~ m|([^/]+)$|)[0]);
                        } else {
                            require File::Copy; File::Copy::copy($SHFILE, $PROGRAM_NAME);
                        }
                    } else {
                        croak "cp: cannot stat '$SHFILE': No such file or directory\n";
                    }
;
                    say "ALSA-Info script has been updated to v $REMOTE_VERSION";
                    say "Please re-run the script";
                    do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
if ( -e "$SHFILE" ) {
                            if ( -d "$SHFILE" ) {
                                croak "rm: ", $SHFILE,
          " is a directory (use -r to remove recursively)\n";
                            }
                            else {
                                if ( unlink "$SHFILE" ) {
                                                                    }
                                else {
                                    croak "rm: cannot remove ", $SHFILE,
              ": $OS_ERROR\n";
                                }
                            }
                        }
                        else {
                            local $CHILD_ERROR = 1;
                            croak "rm: ", $SHFILE, ": No such file or directory\n";
                        }
                    };
}
                else {
                    say "ALSA-Info script has been downloaded as $SHFILE.";
                    say "Please re-run the script from new location.";
                }
exit $main_exit_code;
}
            else {
                do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
if ( -e "$SHFILE" ) {
                        if ( -d "$SHFILE" ) {
                            croak "rm: ", $SHFILE,
          " is a directory (use -r to remove recursively)\n";
                        }
                        else {
                            if ( unlink "$SHFILE" ) {
                                                            }
                            else {
                                croak "rm: cannot remove ", $SHFILE,
              ": $OS_ERROR\n";
                            }
                        }
                    }
                    else {
                        local $CHILD_ERROR = 1;
                        croak "rm: ", $SHFILE, ": No such file or directory\n";
                    }
                };
            }
}
        else {
            say "Newer version detected: $REMOTE_VERSION";
            say "To view the ChangeLog, please visit $CHANGELOG";
if ((-w $0)) {
                say "The original file $PROGRAM_NAME will be overwritten!";
                print "If you do not like to proceed, press Ctrl-C now..";
$inp = <>;
chomp $inp;
$CHILD_ERROR = defined($inp) ? 0 : 1;
                use File::Copy qw(copy);
                if ( -e $SHFILE ) {
                    if ( -d $PROGRAM_NAME ) {
                        require File::Copy; File::Copy::copy($SHFILE, $PROGRAM_NAME . '/' . ($SHFILE =~ m|([^/]+)$|)[0]);
                    } else {
                        require File::Copy; File::Copy::copy($SHFILE, $PROGRAM_NAME);
                    }
                } else {
                    croak "cp: cannot stat '$SHFILE': No such file or directory\n";
                }
;
                say "ALSA-Info script has been updated. Please re-run it.";
                do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
if ( -e "$SHFILE" ) {
                        if ( -d "$SHFILE" ) {
                            croak "rm: ", $SHFILE,
          " is a directory (use -r to remove recursively)\n";
                        }
                        else {
                            if ( unlink "$SHFILE" ) {
                                                            }
                            else {
                                croak "rm: cannot remove ", $SHFILE,
              ": $OS_ERROR\n";
                            }
                        }
                    }
                    else {
                        local $CHILD_ERROR = 1;
                        croak "rm: ", $SHFILE, ": No such file or directory\n";
                    }
                };
}
            else {
                say "ALSA-Info script has been downloaded $SHFILE.";
                say "Please, re-run it from new location.";
            }
exit $main_exit_code;
        }
}
    else {
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
if ( -e "$SHFILE" ) {
                if ( -d "$SHFILE" ) {
                    croak "rm: ", $SHFILE,
          " is a directory (use -r to remove recursively)\n";
                }
                else {
                    if ( unlink "$SHFILE" ) {
                                            }
                    else {
                        croak "rm: cannot remove ", $SHFILE,
              ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: ", $SHFILE, ": No such file or directory\n";
            }
        };
    }
;
    return;
}

sub cleanup {
if (("$TEMPDIR" ne q{} && "$KEEP_FILES" ne "yes")) {
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
if ( -e "$TEMPDIR" ) {
                if ( -d "$TEMPDIR" ) {
                    my $err;
                    require File::Path;
                    File::Path::remove_tree("$TEMPDIR", {error => \$err});
                    if (@{$err}) {
                        carp "rm: carping: could not remove ", "$TEMPDIR", ": $err->[0]\n";
                    }
                    else {
                                            }
                }
                else {
                    if ( unlink "$TEMPDIR" ) {
                                            }
                    else {
                        carp "rm: carping: could not remove ", "$TEMPDIR",
              ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 0;
            }
        };
    }
        $main_exit_code = system('test', '-n', "$ENV{KEEP_OUTPUT}") >> 8;
    if ($CHILD_ERROR != 0) {
                unlink('$NFILE');
    }
;
    return;
}

sub withaplay {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Aplay/Arecord output";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!--------------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "APLAY";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('aplay', '-l') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "ARECORD";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('arecord', '-l') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub withmodules {
    my ($file) = @_;
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!All Loaded Modules";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!------------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    # Original bash: awk '{print $_[0]}' < /proc/modules | sort >> "$FILE"
do {
        my $output_7 = q{};
        my $output_printed_7;
        my $pipeline_success_7 = 1;
                $output = q{};
        open STDIN, '<', '/proc/modules' or croak "Cannot read file: $OS_ERROR\n";
my $tmp_redirect_8 = q{};
my @lines = split /\n/, $output_7;
my @result;
foreach my $line (@lines) {
    chomp $line;
    if ($line =~ /^\s*$/msx) { next; }
    my @fields = split /\s+/msx, $line;
    push @result, ($fields[0] . "\n");
}
$output_7 = join "", @result;

$tmp_redirect_8;
        $output_7 = $output;

                do {
        open my $original_stdout, '>&', STDOUT
        or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
        or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        my $tmp_redirect_10 = q{};
        my @sort_lines_11 = split /\n/, $output_7;
        my @sort_sorted_11 = sort @sort_lines_11;
        $tmp_redirect_10 = join("\n", @sort_sorted_11);
        $output_7 = $tmp_redirect_10;
        $tmp_redirect_10;
        };
        print $tmp;
        if ($tmp eq q{}) { print $output_7; }
        $output_printed_7 = 1;
        open STDOUT, '>&', $original_stdout
        or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
        or die "Close failed: $OS_ERROR\n";
        };
        if ( !$pipeline_success_7 ) { $main_exit_code = 1; }
        }
;
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub withamixer {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Amixer output";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!-------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    my $f;
    for my $f ('/proc/asound/card*/id') {
                if ((-f "$f")) {
            open STDIN, '<', "$f" or croak "Cannot read file: $OS_ERROR\n";
$CARD_NAME = <>;
chomp $CARD_NAME;
$CHILD_ERROR = defined($CARD_NAME) ? 0 : 1;
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
            next;
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!-------Mixer controls for card $ENV{CARD_NAME}";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('amixer', '-c', "$ENV{CARD_NAME}", 'info') >> 8;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('amixer', '-c', "$ENV{CARD_NAME}") >> 8;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
;
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub withalsactl {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Alsactl output";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!--------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    $main_exit_code = system('alsactl', '-f', "$TEMPDIR/alsactl.tmp", 'store') >> 8;
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "--startcollapse--";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "$TEMPDIR/alsactl.tmp" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$TEMPDIR/alsactl.tmp" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "--endcollapse--";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub withdevices {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!ALSA Device nodes";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!-----------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my @ls_files_14 = ();
        my $ls_all_found_15 = 1;
        my @ls_inputs_16 = ();
        my @ls_glob_ls_inputs_16_0 = glob('/dev/snd/*');
        if ( !@ls_glob_ls_inputs_16_0 ) {
            push @ls_inputs_16, '/dev/snd/*';
            $ls_all_found_15 = 0;
        } else {
            push @ls_inputs_16, @ls_glob_ls_inputs_16_0;
        }
        my @ls_files_17 = ();
        my @ls_dirs_18 = ();
        my $ls_show_headers_19 = scalar(@ls_inputs_16) > 1;
        for my $ls_item_20 (@ls_inputs_16) {
            if ( -f $ls_item_20 ) {
                push @ls_files_17, $ls_item_20;
            }
            elsif ( -d $ls_item_20 ) {
                push @ls_dirs_18, $ls_item_20;
            }
            else {
                $ls_all_found_15 = 0;
            }
        }
        @ls_files_17 = sort { $a cmp $b } @ls_files_17;
        @ls_dirs_18 = sort { $a cmp $b } @ls_dirs_18;
        if (@ls_files_17) {
            push @ls_files_14, join("\n", @ls_files_17);
        }
        for my $ls_dir_21 (@ls_dirs_18) {
            my @ls_dir_entries_22 = ();
            if ( opendir my $dh, $ls_dir_21 ) {
                while ( my $file = readdir $dh ) {
                    push @ls_dir_entries_22, $file;
                }
                closedir $dh;
                @ls_dir_entries_22 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_dir_entries_22;
                if ( $ls_show_headers_19 ) {
                    if ( @ls_dir_entries_22 ) {
                        push @ls_files_14, $ls_dir_21 . ":\n" . join("\n", @ls_dir_entries_22);
                    } else {
                        push @ls_files_14, $ls_dir_21 . ':';
                    }
                }
                elsif ( @ls_dir_entries_22 ) {
                    push @ls_files_14, join("\n", @ls_dir_entries_22);
                }
            }
            else {
                $ls_all_found_15 = 0;
            }
        }
        if (@ls_files_14) {
            print join "\n", @ls_files_14;
            print "\n";
        }
        if ( $ls_all_found_15 ) {
            local $CHILD_ERROR = 0;
            $ls_success = 1;
        }
        else {
            local $CHILD_ERROR = 2;
            $ls_success = 0;
            $main_exit_code = $CHILD_ERROR;
        }
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub withconfigs {
if ((((-e "$HOME/.asoundrc") || (-e "/etc/asound.conf")) || (-e "$HOME/.asoundrc.asoundconf"))) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!ALSA configuration files";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!------------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
if ((-e "$HOME/.asoundrc")) {
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "!!User specific config file (~/.asoundrc)";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "$ENV{HOME}/.asoundrc" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$ENV{HOME}/.asoundrc" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
        }
if ((-e "$HOME/.asoundrc.asoundconf")) {
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "!!asoundconf-generated config file";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "$ENV{HOME}/.asoundrc.asoundconf" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$ENV{HOME}/.asoundrc.asoundconf" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
        }
if ((-e '/etc/asound.conf')) {
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "!!System wide config file (/etc/asound.conf)";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', '/etc/asound.conf' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/etc/asound.conf' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
        }
    }
    return;
}

sub withsysfs {
    my $i;
    my $f;
    my $printed = "";
    for my $i ('/sys/class/sound/*') {
if ("$i" =~ /^.*/hwC.D.$/msx) {
            if ((-f "$i/init_pin_configs")) {
if ("$printed" eq q{}) {
                    do {
                        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                        my $tmp = do {
                        say "!!Sysfs Files";
                        };
                        print $tmp;
                        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                    };
                    do {
                        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                        my $tmp = do {
                        say "!!-----------";
                        };
                        print $tmp;
                        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                    };
                    do {
                        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                        my $tmp = do {
                        say "";
                        };
                        print $tmp;
                        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                    };
                }
                for my $f ('init_pin_configs', 'driver_pin_configs', 'user_pin_configs', 'init_verbs', 'hints') {
                    do {
                        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                        my $tmp = do {
                        say "$i/$f:";
                        };
                        print $tmp;
                        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                    };
                    do {
                        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "$i/$f" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$i/$f" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                    };
                    do {
                        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
                        print "\n";
                        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                    };
                }
                $printed = 'yes';
            }
        }
    }
if ("$printed" ne q{}) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
    return;
}

sub withdmesg {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!ALSA/HDA dmesg";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!--------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    # Original bash: dmesg | grep -C1 -E 'ALSA|HDA|HDMI|snd[_-]|sound|audio|hda.codec|hda.intel' >> "$FILE"
do {
        my $output_27 = q{};
        my $output_printed_27;
        my $pipeline_success_27 = 1;
                my ($in_28, $out_28);
        my $pid_28 = open3($in_28, $out_28, '>&STDERR', 'dmesg', );
        close $in_28 or croak 'Close failed: $OS_ERROR';
        $output_27 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_28> };
        close $out_28 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_28, 0;

                do {
        open my $original_stdout, '>&', STDOUT
        or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
        or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        my $tmp_redirect_29 = q{};
        my $grep_result_30;
        my @grep_lines_30 = split /\n/msx, $output_27;
        my @grep_filtered_30 = grep { /ALSA|HDA|HDMI|snd[_-]|sound|audio|hda.codec|hda.intel/msx } @grep_lines_30;
        $grep_result_30 = join "\n", @grep_filtered_30;
        if (!($grep_result_30 =~ m{\n\z} || $grep_result_30 eq q{})) {
        $grep_result_30 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_30 > 0 ? 0 : 1;
        $tmp_redirect_29 = $grep_result_30;
        $tmp_redirect_29;
        };
        print $tmp;
        if ($tmp eq q{}) { print $output_27; }
        $output_printed_27 = 1;
        open STDOUT, '>&', $original_stdout
        or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
        or die "Close failed: $OS_ERROR\n";
        };
        if ( !$pipeline_success_27 ) { $main_exit_code = 1; }
        }
;
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub withpackages {
    my $RPM;
    my $DPKG;
    $RPM = (do {
    my ($in_31, $out_31);
    my $pid_31 = open3($in_31, $out_31, '>&STDERR', 'command', '-v', 'rpmquery');
    close $in_31 or croak 'Close failed: $OS_ERROR';
    my $result_31 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_31> };
    close $out_31 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_31, 0;
    $result_31
});
    $DPKG = (do {
    my ($in_32, $out_32);
    my $pid_32 = open3($in_32, $out_32, '>&STDERR', 'command', '-v', 'dpkg');
    close $in_32 or croak 'Close failed: $OS_ERROR';
    my $result_32 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_32> };
    close $out_32 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_32, 0;
    $result_32
});
    if (!("$RPM$DPKG" ne q{})) {
        return;
    }
    my $PATTERN = "(alsa-(lib|oss|plugins|tools|(topology|ucm)-conf|utils|sof-firmware)|libalsa|tinycompress|sof-firmware)";
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', "$ENV{FILE}"
      or die "Cannot access file: $OS_ERROR\n";
            say "!!Packages installed";
            say "!!--------------------";
            say "";
            # Original bash: #!/bin/bash
do {
                my $output_33 = q{};
                my $output_printed_33;
                my $pipeline_success_33 = 1;
                                my @_pcmd_35 = ('bash', '-c', ": \"Complex command cannot be converted to shell command\"");
                my ($in_34);
                my $pid_34 = open3($in_34, $out_34, '>&STDERR', @_pcmd_35);
                close $in_34 or croak 'Close failed: $OS_ERROR';
                my $temp_result;
                $temp_result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_34> };
                $output_33 = $temp_result;
                close $out_34 or croak 'Close failed: $OS_ERROR';
                waitpid $pid_34, 0;

                                my $grep_result_33_1;
                my @grep_lines_33_1 = split /\n/msx, $output_33;
                my @grep_filtered_33_1 = grep { /$PATTERN/msx } @grep_lines_33_1;
                $grep_result_33_1 = join "\n", @grep_filtered_33_1;
                if (!($grep_result_33_1 =~ m{\n\z} || $grep_result_33_1 eq q{})) {
                $grep_result_33_1 .= "\n";
                }
                $CHILD_ERROR = scalar @grep_filtered_33_1 > 0 ? 0 : 1;
                $output_33 = $grep_result_33_1;
                $output_33 = $grep_result_33_1;
                if ((scalar @grep_filtered_33_1) == 0) {
                    $pipeline_success_33 = 0;
                }
                if ($output_33 ne q{} && !defined $output_printed_33) {
                    print $output_33;
                    if (!($output_33 =~ m{\n\z})) {
                        print "\n";
                    }
                }
                if ( !$pipeline_success_33 ) { $main_exit_code = 1; }
                }
;
            say "";
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub withall {
    withdevices();
    withconfigs();
    withaplay();
    withamixer();
    withalsactl();
    withmodules();
    withsysfs();
    withdmesg();
    withpackages();
    $WITHALL = 'no';
    return;
}

sub get_alsa_library_version {
    my ($file) = @_;
    my $ALSA_LIB_VERSION = (do { my $result_36 = qx{bash -c q(grep VERSION_STR /usr/include/alsa/version.h 2> /dev/null | awk '{ print $_[2] }' | sed 's/"//g') }; chomp $result_36; $result_36; });
if ("$ALSA_LIB_VERSION" eq q{}) {
if ((-f '/etc/lsb-release')) {
            $main_exit_code = system('.', '/etc/lsb-release') >> 8;
if ("$ENV{DISTRIB_ID}" eq 'Ubuntu') {
                if (!(                do {
                    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                    open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
                    my $tmp = do {
                    $main_exit_code = system('command', '-v', 'dpkg') >> 8;
                    };
                    print $tmp;
                    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                };)) {
                    $ALSA_LIB_VERSION = (do { my $result_37 = qx{bash -c q(dpkg -l libasound2 | tail -1 | awk '{ print $_[2] }' | cut -f 1 -d -) }; chomp $result_37; $result_37; });
                }
                if ("$ALSA_LIB_VERSION" eq '<none>') {
                    $ALSA_LIB_VERSION = "";
                }
                return;            } elsif (1) {
                return;            }
}
        else {
            if ((-f '/etc/debian_version')) {
if (!(                do {
                    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                    open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
                    my $tmp = do {
                    $main_exit_code = system('command', '-v', 'dpkg') >> 8;
                    };
                    print $tmp;
                    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                };)) {
                    $ALSA_LIB_VERSION = (do { my $result_38 = qx{bash -c q(dpkg -l libasound2 | tail -1 | awk '{ print $_[2] }' | cut -f 1 -d -) }; chomp $result_38; $result_38; });
                }
if ("$ALSA_LIB_VERSION" eq '<none>') {
                    $ALSA_LIB_VERSION = "";
                }
return;
            }
        }
    }
    return;
}
my $t;
my $prg;
for my $prg (@REQUIRES) {
    $t = "$(command -v ";
    $CHILD_ERROR = 0;
if (StringInterpolation(StringInterpolation { parts: [Variable("t")] }, None) eq q{}) {
        say "This script requires $prg utility to continue.";
exit 1;
    }
}
$LSPCI = (do {
    my ($in_39, $out_39);
    my $pid_39 = open3($in_39, $out_39, '>&STDERR', 'command', '-v', 'lspci');
    close $in_39 or croak 'Close failed: $OS_ERROR';
    my $result_39 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_39> };
    close $out_39 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_39, 0;
    $result_39
});
$TPUT = (do {
    my ($in_40, $out_40);
    my $pid_40 = open3($in_40, $out_40, '>&STDERR', 'command', '-v', 'tput');
    close $in_40 or croak 'Close failed: $OS_ERROR';
    my $result_40 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_40> };
    close $out_40 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_40, 0;
    $result_40
});
$DIALOG = (do {
    my ($in_41, $out_41);
    my $pid_41 = open3($in_41, $out_41, '>&STDERR', 'command', '-v', 'dialog');
    close $in_41 or croak 'Close failed: $OS_ERROR';
    my $result_41 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_41> };
    close $out_41 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_41, 0;
    $result_41
});
$SYSFS = (do { my $result_42 = qx{bash -c q(mount | grep sysfs | awk '{ print $3 }') }; chomp $result_42; $result_42; });
$SNDOPTIONS = (do { my $result_43 = qx{bash -c 'modprobe -c | sed -n "s/^options \\\\(snd[-_][^ ]*\\\\)/\\\\1:/p"' }; chomp $result_43; $result_43; });
my $KEEP_OUTPUT = q{};
$NFILE = "";
$PASTEBIN = "";
my $WWWSERVICE = 'www.alsa-project.org';
$WELCOME = 'yes';
$PROCEED = 'yes';
$UPLOAD = 'ask';
$REPEAT = "";
while ( "$REPEAT" eq q{} ) {
    $REPEAT = 'no';
if ("$_[0]" eq '--update' or "$_[0]" eq '--help' or "$_[0]" eq '--about') {
                $WELCOME = 'no';
                $PROCEED = 'no';
    } elsif ("$_[0]" eq '--upload') {
                $UPLOAD = 'yes';
                $WELCOME = 'no';
    } elsif ("$_[0]" eq '--no-upload') {
                $UPLOAD = 'no';
                $WELCOME = 'no';
    } elsif ("$_[0]" eq '--pastebin') {
                $PASTEBIN = 'yes';
                $WWWSERVICE = 'pastebin';
    } elsif ("$_[0]" eq '--no-dialog') {
                $DIALOG = "";
                $REPEAT = "";
        # Builtin command 'shift' not implemented
    } elsif ("$_[0]" eq '--stdout') {
                $DIALOG = "";
                $WELCOME = 'no';
    }
}
my $greeting_message;
if ("$WELCOME" eq yes) {
    $greeting_message = "\

This script visits the following commands/files to collect diagnostic
information about your ALSA installation and sound related hardware.

  dmesg
  lspci
  aplay
  amixer
  alsactl
  rpm, dpkg
  /proc/asound/
  /sys/class/sound/
  ~/.asoundrc (etc.)

See '$PROGRAM_NAME --help' for command line options.
";
if ("$DIALOG" ne q{}) {
        $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--title', "ALSA-Info script v $SCRIPT_VERSION", '--msgbox', "$greeting_message", '20', '80') >> 8;
}
    else {
        say "ALSA Information Script v $SCRIPT_VERSION";
        say "--------------------------------";
        say $greeting_message;
    }
}
$TEMPDIR = (do {
    my ($in_44, $out_44);
    my $pid_44 = open3($in_44, $out_44, '>&STDERR', 'mktemp', '-t', '-d', 'alsa-info.XXXXXXXXXX');
    close $in_44 or croak 'Close failed: $OS_ERROR';
    my $result_44 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_44> };
    close $out_44 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_44, 0;
    $result_44
});
if ($CHILD_ERROR != 0) {
    exit 1;
}
my $FILE = "$TEMPDIR/alsa-info.txt";
if ("$NFILE" eq q{}) {
        $NFILE = (do {
    my ($in_45, $out_45);
    my $pid_45 = open3($in_45, $out_45, '>&STDERR', 'mktemp', '-t', 'alsa-info.txt.XXXXXXXXXX');
    close $in_45 or croak 'Close failed: $OS_ERROR';
    my $result_45 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_45> };
    close $out_45 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_45, 0;
    $result_45
});
    if ($CHILD_ERROR != 0) {
        exit 1;
    }
;
}
END { local $INPUT_RECORD_SEPARATOR = undef; my $end_out = qx'cleanup 2>&1'; print $end_out if $end_out ne q{}; }
my $DISTRO;
my $ROARRUNNING;
my $DMI_SYSTEM_MANUFACTURER;
my $ESDRUNNING;
my $JACK2RUNNING;
my $DMI_SYSTEM_PRODUCT_VERSION;
my $DMI_SYSTEM_FIRMWARE_VERSION;
my $value;
my $driver;
my $ALSA_UTILS_VERSION;
my $list;
my $DMI_BOARD_NAME;
my $ARTSRUNNING;
my $JACKRUNNING;
my $KERNEL_SMP;
my $PARUNNING;
my $DMI_SYSTEM_PRODUCT_NAME;
my $PWRUNNING;
my $TSTAMP;
my $DMI_BOARD_VENDOR;
my $id;
my $DMI_SYSTEM_SKU;
my $ALSA_DRIVER_VERSION;
if ("$PROCEED" eq yes) {
if ("$LSPCI" eq q{}) {
if ((-d '/sys/bus/pci')) {
            say "This script requires lspci. Please install it, and re-run this script.";
        }
    }
    $TSTAMP = do {
local $ENV{LANG} = q{C};
local $ENV{TZ} = 'UTC';
require POSIX; POSIX::strftime('%a %b %e %H:%M:%S %Z %Y', localtime())
};
    $DISTRO = do { my $grep_result_46;
my @grep_lines_46 = ();
my @grep_filenames_46 = ();
if (-e "/etc/") {
    open my $fh, '<', "/etc/" or croak "Cannot access file: $ERRNO";
    while (my $line = <$fh>) {
        chomp $line;
        push @grep_lines_46, $line;
        push @grep_filenames_46, "/etc/";
    }
    close $fh
        or croak "Close failed: $OS_ERROR";
}
else { print {*STDERR} "grep: /etc/: No such file or directory\n"; }
my @grep_filtered_46 = grep { /buntu|SUSE|Fedora|PCLinuxOS|MEPIS|Mandriva|Debian|Damn|Sabayon|Slackware|KNOPPIX|Gentoo|Zenwalk|Mint|Kubuntu|FreeBSD|Puppy|Freespire|Vector|Dreamlinux|CentOS|Arch|Xandros|Elive|SLAX|Red|BSD|KANOTIX|Nexenta|Foresight|GeeXboX|Frugalware|64|SystemRescue|Novell|Solaris|BackTrack|KateOS|Pardus|ALT/msxi } @grep_lines_46;
$grep_result_46 = join "\n", @grep_filtered_46;
    if (!($grep_result_46 =~ m{\n\z} || $grep_result_46 eq q{})) {
        $grep_result_46 .= "\n";
    }
$CHILD_ERROR = scalar @grep_filtered_46 > 0 ? 0 : 1;
 $grep_result_46; };
    my $temp_file_ps_fh_1 = q{/tmp} . '/process_sub_fh_1.tmp';
    my $output_ps_fh_1;
    {
        local *STDOUT;
        open STDOUT, '>', \$output_ps_fh_1 or croak "Cannot redirect STDOUT";
        my $output_47 = q{};
        my $output_printed_47;
        $main_exit_code = system('/bin/uname', '-r', 'pmo') >> 8;
    if ($output_47 ne q{} && !$output_printed_47) {
        print $output_47;
    }
    }
    use File::Path qw(make_path);
    my $temp_dir_fh_1 = dirname($temp_file_ps_fh_1);
    if (!-d $temp_dir_fh_1) { make_path($temp_dir_fh_1); }
    open my $fh_ps_fh_1, '>', $temp_file_ps_fh_1 or croak "Cannot create temp file: $ERRNO\n";
    print {$fh_ps_fh_1} $output_ps_fh_1;
    close $fh_ps_fh_1 or croak "Close failed: $ERRNO\n";
    open STDIN, '<', $temp_file_ps_fh_1 or croak "Cannot open process substitution: $ERRNO\n";
$KERNEL_RELEASE = <>;
chomp $KERNEL_RELEASE;
$CHILD_ERROR = defined($KERNEL_RELEASE) ? 0 : 1;
    my $temp_file_ps_fh_2 = q{/tmp} . '/process_sub_fh_2.tmp';
    my $output_ps_fh_2;
    {
        local *STDOUT;
        open STDOUT, '>', \$output_ps_fh_2 or croak "Cannot redirect STDOUT";
        my $output_49 = q{};
        my $output_printed_49;
        $main_exit_code = system('/bin/uname', '-v') >> 8;
    if ($output_49 ne q{} && !$output_printed_49) {
        print $output_49;
    }
    }
    use File::Path qw(make_path);
    my $temp_dir_fh_2 = dirname($temp_file_ps_fh_2);
    if (!-d $temp_dir_fh_2) { make_path($temp_dir_fh_2); }
    open my $fh_ps_fh_2, '>', $temp_file_ps_fh_2 or croak "Cannot create temp file: $ERRNO\n";
    print {$fh_ps_fh_2} $output_ps_fh_2;
    close $fh_ps_fh_2 or croak "Close failed: $ERRNO\n";
    open STDIN, '<', $temp_file_ps_fh_2 or croak "Cannot open process substitution: $ERRNO\n";
$KERNEL_VERSION = <>;
chomp $KERNEL_VERSION;
$CHILD_ERROR = defined($KERNEL_VERSION) ? 0 : 1;
if ("$KERNEL_VERSION" eq *SMP*) {
        $KERNEL_SMP = 'Yes';
}
    else {
        $KERNEL_SMP = 'No';
    }
    $ALSA_DRIVER_VERSION = do { my $result_51 = qx{bash -c q(cat /proc/asound/version | head -n 1 | awk '{ print $7 }' | sed "s/\\.\$//") }; chomp $result_51; $result_51; };
    get_alsa_library_version();
    $ALSA_UTILS_VERSION = do { my $result_52 = qx{bash -c q(amixer -v | awk '{ print $3 }') }; chomp $result_52; $result_52; };
    $ESDINST = do {
    my ($in_53, $out_53);
    my $pid_53 = open3($in_53, $out_53, '>&STDERR', 'command', '-v', 'esd');
    close $in_53 or croak 'Close failed: $OS_ERROR';
    my $result_53 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_53> };
    close $out_53 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_53, 0;
    $result_53
};
    $PWINST = do {
    my ($in_54, $out_54);
    my $pid_54 = open3($in_54, $out_54, '>&STDERR', 'command', '-v', 'pipewire');
    close $in_54 or croak 'Close failed: $OS_ERROR';
    my $result_54 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_54> };
    close $out_54 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_54, 0;
    $result_54
};
    $PAINST = do {
    my ($in_55, $out_55);
    my $pid_55 = open3($in_55, $out_55, '>&STDERR', 'command', '-v', 'pulseaudio');
    close $in_55 or croak 'Close failed: $OS_ERROR';
    my $result_55 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_55> };
    close $out_55 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_55, 0;
    $result_55
};
    $ARTSINST = do {
    my ($in_56, $out_56);
    my $pid_56 = open3($in_56, $out_56, '>&STDERR', 'command', '-v', 'artsd');
    close $in_56 or croak 'Close failed: $OS_ERROR';
    my $result_56 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_56> };
    close $out_56 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_56, 0;
    $result_56
};
    $JACKINST = do {
    my ($in_57, $out_57);
    my $pid_57 = open3($in_57, $out_57, '>&STDERR', 'command', '-v', 'jackd');
    close $in_57 or croak 'Close failed: $OS_ERROR';
    my $result_57 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_57> };
    close $out_57 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_57, 0;
    $result_57
};
    $JACK2INST = do {
    my ($in_58, $out_58);
    my $pid_58 = open3($in_58, $out_58, '>&STDERR', 'command', '-v', 'jackdbus');
    close $in_58 or croak 'Close failed: $OS_ERROR';
    my $result_58 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_58> };
    close $out_58 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_58, 0;
    $result_58
};
    $ROARINST = do {
    my ($in_59, $out_59);
    my $pid_59 = open3($in_59, $out_59, '>&STDERR', 'command', '-v', 'roard');
    close $in_59 or croak 'Close failed: $OS_ERROR';
    my $result_59 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_59> };
    close $out_59 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_59, 0;
    $result_59
};
    $DMIDECODE = do {
    my ($in_60, $out_60);
    my $pid_60 = open3($in_60, $out_60, '>&STDERR', 'command', '-v', 'dmidecode');
    close $in_60 or croak 'Close failed: $OS_ERROR';
    my $result_60 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_60> };
    close $out_60 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_60, 0;
    $result_60
};
if ((-d '/sys/class/dmi/id')) {
        $DMI_SYSTEM_MANUFACTURER = do { my @_qx_cmd = ("cat /sys/class/dmi/id/sys_vendor 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $DMI_SYSTEM_PRODUCT_NAME = do { my @_qx_cmd = ("cat /sys/class/dmi/id/product_name 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $DMI_SYSTEM_PRODUCT_VERSION = do { my @_qx_cmd = ("cat /sys/class/dmi/id/product_version 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $DMI_SYSTEM_FIRMWARE_VERSION = do { my @_qx_cmd = ("cat /sys/class/dmi/id/bios_version 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $DMI_SYSTEM_SKU = do { my @_qx_cmd = ("cat /sys/class/dmi/id/product_sku 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $DMI_BOARD_VENDOR = do { my @_qx_cmd = ("cat /sys/class/dmi/id/board_vendor 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $DMI_BOARD_NAME = do { my @_qx_cmd = ("cat /sys/class/dmi/id/board_name 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
}
    else {
        if ((-x $DMIDECODE)) {
            $DMI_SYSTEM_MANUFACTURER = do { my @_qx_cmd = ("$DMIDECODE -s system-manufacturer 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            $DMI_SYSTEM_PRODUCT_NAME = do { my @_qx_cmd = ("$DMIDECODE -s system-product-name 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            $DMI_SYSTEM_PRODUCT_VERSION = do { my @_qx_cmd = ("$DMIDECODE -s system-version 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            $DMI_SYSTEM_FIRMWARE_VERSION = do { my @_qx_cmd = ("$DMIDECODE -s bios-version 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            $DMI_SYSTEM_SKU = do { my @_qx_cmd = ("$DMIDECODE -s system-sku-number 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            $DMI_BOARD_VENDOR = do { my @_qx_cmd = ("$DMIDECODE -s baseboard-manufacturer 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            $DMI_BOARD_NAME = do { my @_qx_cmd = ("$DMIDECODE -s baseboard-product-name 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        }
    }
if ((-d '/sys/bus/acpi/devices')) {
        my $f;
        for my $f ('/sys/bus/acpi/devices/*/status') {
            $ACPI_STATUS = do { my @_qx_cmd = ("cat Variable(\"f\", false, None) 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if (($ACPI_STATUS != 0)) {
                do {
                    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                    open STDOUT, '>>', $TEMPDIR
      or die "Cannot access file: $OS_ERROR\n";
                    my $tmp = do {
                    say $f . q{ } . "\t" . q{ } . $ACPI_STATUS . q{ } . '/acpidevicestatus.tmp';
                    };
                    print $tmp;
                    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                };
            }
        }
;
    }
open STDIN, '<', '/proc/asound/modules' or croak "Cannot read file: $OS_ERROR\n";
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', $TEMPDIR
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
my @lines = split /\n/, $;
my @result;
foreach my $line (@lines) {
    chomp $line;
    if ($line =~ /^\s*$/msx) { next; }
    my @fields = split /\s+/msx, $line;
    push @result, ($fields[1] . " (card " . $fields[0] . ")" . "\n");
}
$ = join "", @result;

        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', $TEMPDIR
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', '/proc/asound/cards' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/proc/asound/cards' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsacards.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsacards.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
if (! "$LSPCI" eq q{}) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $TEMPDIR
      or die "Cannot access file: $OS_ERROR\n";
            my $class;
            for my $class (q{0401}, q{0402}, q{0403}) {
                # Original bash: lspci -vvnn -d "::$class" | sed -n '/^[^\t]/,+1p'
do {
                    my $output_63 = q{};
                    my $output_printed_63;
                    my $pipeline_success_63 = 1;
                                        my ($in_64, $out_64);
                    my $pid_64 = open3($in_64, $out_64, '>&STDERR', 'lspci', '-vvnn', '-d');
                    close $in_64 or croak 'Close failed: $OS_ERROR';
                    $output_63 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_64> };
                    close $out_64 or croak 'Close failed: $OS_ERROR';
                    waitpid $pid_64, 0;

                                        my @sed_lines_63 = split /\n/, $output_63;
                    my @sed_result_63;
                    foreach my $line (@sed_lines_63) {
                    chomp $line;
                    push @sed_result_63, $line;
                    }
                    $output_63 = join "\n", @sed_result_63;
                    if ($output_63 ne q{} && !defined $output_printed_63) {
                        print $output_63;
                        if (!($output_63 =~ m{\n\z})) {
                            print "\n";
                        }
                    }
                    if ( !$pipeline_success_63 ) { $main_exit_code = 1; }
                    }
;
            }
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        $main_exit_code = system('bash', '/lspci.tmp') >> 8;
    }
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "/proc/asound/card*/codec\\#* > \$TEMPDIR/alsa-hda-intel.tmp 2> /dev/null" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "/proc/asound/card*/codec\\#* > \$TEMPDIR/alsa-hda-intel.tmp 2> /dev/null" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "/proc/asound/card*/codec97\\#0/ac97\\#0-0 > \$TEMPDIR/alsa-ac97.tmp 2> /dev/null" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "/proc/asound/card*/codec97\\#0/ac97\\#0-0 > \$TEMPDIR/alsa-ac97.tmp 2> /dev/null" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "/proc/asound/card*/codec97\\#0/ac97\\#0-0+regs > \$TEMPDIR/alsa-ac97-regs.tmp 2> /dev/null" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "/proc/asound/card*/codec97\\#0/ac97\\#0-0+regs > \$TEMPDIR/alsa-ac97-regs.tmp 2> /dev/null" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
if ((-x '/usr/bin/lsusb')) {
        for my $f ('/proc/asound/card[0-9]*/usbbus') {
                        $main_exit_code = system('test', '-f', "$f") >> 8;
            if ($CHILD_ERROR != 0) {
                next;
            }
;
            $id = do { my @_qx_cmd = ('sed s@/@:@ $f'); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $TEMPDIR
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                $main_exit_code = system('lsusb', '-v', '-s', $id, '/lsusb.tmp') >> 8;
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
        }
    }
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', $TEMPDIR
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', '/proc/asound/card*/stream[0-9]*' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/proc/asound/card*/stream[0-9]*' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsa-usbstream.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsa-usbstream.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', $TEMPDIR
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', '/proc/asound/card*/usbmixer' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/proc/asound/card*/usbmixer' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsa-usbmixer.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsa-usbmixer.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
if ($PASTEBIN eq q{}) {
        open my $fh, '>', '$FILE' or die "$FILE: $!\n";
        say {*fh} "upload=true&script=true&cardinfo=";
        close $fh;
}
    else {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "name=$ENV{USER}&type=33&description=/tmp/alsa-info.txt&expiry=&s=Submit+Post&content=";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!################################";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!ALSA Information Script v $SCRIPT_VERSION";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!################################";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Script ran on: $TSTAMP";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Linux Distribution";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!------------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say $DISTRO;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!DMI Information";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!---------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Manufacturer:      $DMI_SYSTEM_MANUFACTURER";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Product Name:      $DMI_SYSTEM_PRODUCT_NAME";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Product Version:   $DMI_SYSTEM_PRODUCT_VERSION";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Firmware Version:  $DMI_SYSTEM_FIRMWARE_VERSION";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "System SKU:        $DMI_SYSTEM_SKU";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Board Vendor:      $DMI_BOARD_VENDOR";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Board Name:        $DMI_BOARD_NAME";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!ACPI Device Status Information";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!---------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/acpidevicestatus.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/acpidevicestatus.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Kernel Information";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!------------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Kernel release:    $KERNEL_VERSION";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Operating System:  $ENV{KERNEL_OS}";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Architecture:      $ENV{KERNEL_MACHINE}";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Processor:         $ENV{KERNEL_PROCESSOR}";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "SMP Enabled:       $KERNEL_SMP";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!ALSA Version";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Driver version:     $ALSA_DRIVER_VERSION";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Library version:    $ENV{ALSA_LIB_VERSION}";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "Utilities version:  $ALSA_UTILS_VERSION";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Loaded ALSA modules";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!-------------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsamodules.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsamodules.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Sound Servers on this " . "sys" . "tem";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!----------------------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
if ($PWINST ne q{}) {
                if ((qx'pgrep '^(.*/)?pipewire$'' ne q{})) {
                        $PWRUNNING = "Yes";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
                        $PWRUNNING = "No";
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "PipeWire:";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Installed - Yes ($PWINST)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Running - $PWRUNNING";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if ($PAINST ne q{}) {
                if ((qx'pgrep '^(.*/)?pulseaudio$'' ne q{})) {
                        $PARUNNING = "Yes";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
                        $PARUNNING = "No";
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "Pulseaudio:";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Installed - Yes ($PAINST)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Running - $PARUNNING";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if ($ESDINST ne q{}) {
                if ((qx'pgrep '^(.*/)?esd$'' ne q{})) {
                        $ESDRUNNING = "Yes";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
                        $ESDRUNNING = "No";
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "ESound Daemon:";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Installed - Yes ($ESDINST)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Running - $ESDRUNNING";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if ($ARTSINST ne q{}) {
                if ((qx'pgrep '^(.*/)?artsd$'' ne q{})) {
                        $ARTSRUNNING = "Yes";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
                        $ARTSRUNNING = "No";
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "aRts:";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Installed - Yes ($ARTSINST)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Running - $ARTSRUNNING";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if ($JACKINST ne q{}) {
                if ((qx'pgrep '^(.*/)?jackd$'' ne q{})) {
                        $JACKRUNNING = "Yes";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
                        $JACKRUNNING = "No";
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "Jack:";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Installed - Yes ($JACKINST)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Running - $JACKRUNNING";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if ($JACK2INST ne q{}) {
                if ((qx'pgrep '^(.*/)?jackdbus$'' ne q{})) {
                        $JACK2RUNNING = "Yes";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
                        $JACK2RUNNING = "No";
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "Jack2:";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Installed - Yes ($JACK2INST)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Running - $JACK2RUNNING";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if ($ROARINST ne q{}) {
                if ((qx'pgrep '^(.*/)?roard$'' ne q{})) {
                        $ROARRUNNING = "Yes";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        if ($CHILD_ERROR != 0) {
                        $ROARRUNNING = "No";
        }
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "RoarAudio:";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Installed - Yes ($ROARINST)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "      Running - $ROARRUNNING";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if (0) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "No sound servers found.";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!Soundcards recognised by ALSA";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "!!-----------------------------";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsacards.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsacards.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        say "";
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
if (! "$LSPCI" eq q{}) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!PCI Soundcards installed in the " . "sys" . "tem";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!--------------------------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/lspci.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/lspci.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if (("$SNDOPTIONS")) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!Modprobe options (Sound related)";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!--------------------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        # Original bash: modprobe -c|sed -n 's/^options \(snd[-_][^ ]*\)/\1:/p' >> $FILE
do {
            my $output_74 = q{};
            my $output_printed_74;
            my $pipeline_success_74 = 1;
                        my ($in_75, $out_75);
            my $pid_75 = open3($in_75, $out_75, '>&STDERR', 'modprobe', '-c');
            close $in_75 or croak 'Close failed: $OS_ERROR';
            $output_74 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_75> };
            close $out_75 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_75, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            my $tmp_redirect_76 = q{};
            my @sed_lines_77 = split /\n/, $output_74;
            my @sed_result_77;
            foreach my $line (@sed_lines_77) {
            chomp $line;
            push @sed_result_77, $line;
            }
            $output_74 = join "\n", @sed_result_77;
            $tmp_redirect_76;
            };
            print $tmp;
            if ($tmp eq q{}) { print $output_74; }
            $output_printed_74 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_74 ) { $main_exit_code = 1; }
            }
;
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if ((-d "$SYSFS")) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!Loaded sound module options";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!---------------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        my $mod;
        for my $mod (do { my $result_78 = qx{bash -c q(cat /proc/asound/modules | awk '{ print $2 }') }; chomp $result_78; $result_78; }) {
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "!!Module: $mod";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $params;
                for my $params ($SYSFS . q{ } . '/module/' . q{ } . $mod . q{ } . '/parameters/*') {
                    say '-ne' . q{ } . "\t";
                    $value = do { my $cat_chunk = q{}; if ( open my $fh, '<', $params ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $params . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                    # Original bash: echo "$params : $value" | sed 's:.*/::'
do {
                        my $output_79 = q{};
                        my $output_printed_79;
                        my $pipeline_success_79 = 1;
                        $output_79 .= "$params : $value\n";
if ( !($output_79 =~ m{\n\z}) ) { $output_79 .= "\n"; }

                                                my @sed_lines_79 = split /\n/, $output_79;
                        my @sed_result_79;
                        foreach my $line (@sed_lines_79) {
                        chomp $line;
                        push @sed_result_79, $line;
                        }
                        $output_79 = join "\n", @sed_result_79;
                        if ($output_79 ne q{} && !defined $output_printed_79) {
                            print $output_79;
                            if (!($output_79 =~ m{\n\z})) {
                                print "\n";
                            }
                        }
                        if ( !$pipeline_success_79 ) { $main_exit_code = 1; }
                        }
;
                }
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
        }
;
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!Sysfs card info";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!---------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        my $cdir;
        for my $cdir ($SYSFS . q{ } . '/class/sound/card*') {
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "!!Card: $cdir";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            $driver = do {
    my ($in_80, $out_80);
    my $pid_80 = open3($in_80, $out_80, '>&STDERR', 'readlink', '-f', "$cdir/device/driver");
    close $in_80 or croak 'Close failed: $OS_ERROR';
    my $result_80 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_80> };
    close $out_80 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_80, 0;
    $result_80
};
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "Driver: $driver";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "Tree:";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            # Original bash: tree --noreport $cdir -L 2 | sed -e 's/^/\t/g' >> $FILE
do {
                my $output_81 = q{};
                my $output_printed_81;
                my $pipeline_success_81 = 1;
                                my ($in_82, $out_82);
                my $pid_82 = open3($in_82, $out_82, '>&STDERR', 'tree', '--noreport', '-L', q{2});
                close $in_82 or croak 'Close failed: $OS_ERROR';
                $output_81 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_82> };
                close $out_82 or croak 'Close failed: $OS_ERROR';
                waitpid $pid_82, 0;

                                do {
                open my $original_stdout, '>&', STDOUT
                or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
                or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                my $tmp_redirect_83 = q{};
                my @sed_lines_84 = split /\n/, $output_81;
                my @sed_result_84;
                foreach my $line (@sed_lines_84) {
                chomp $line;
                push @sed_result_84, $line;
                }
                $output_81 = join "\n", @sed_result_84;
                $tmp_redirect_83;
                };
                print $tmp;
                if ($tmp eq q{}) { print $output_81; }
                $output_printed_81 = 1;
                open STDOUT, '>&', $original_stdout
                or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
                or die "Close failed: $OS_ERROR\n";
                };
                if ( !$pipeline_success_81 ) { $main_exit_code = 1; }
                }
;
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
        }
;
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
if ((-d $SYSFS/class/sound/ctl-led)) {
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "!!Sysfs ctl-led info";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "!!---------------";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                say "";
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            for my $path ($SYSFS . q{ } . '/class/sound/ctl-led/[ms][ip]*/card*') {
                do {
                    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                    open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                    my $tmp = do {
                    say "!!CTL-LED: $path";
                    };
                    print $tmp;
                    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                };
if ((-r "$path/list")) {
                    $list = do { my $cat_chunk = q{}; if ( open my $fh, '<', "$path/list" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$path/list" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                    do {
                        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                        open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                        my $tmp = do {
                        say "List: $list";
                        };
                        print $tmp;
                        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                    };
                }
                do {
                    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                    open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
                    my $tmp = do {
                    say "";
                    };
                    print $tmp;
                    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                };
            }
        }
    }
if (((-s "$TEMPDIR/alsa-hda-intel.tmp") > 0)) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!HDA-Intel Codec information";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!---------------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--startcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsa-hda-intel.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsa-hda-intel.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--endcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if (((-s "$TEMPDIR/alsa-ac97.tmp") > 0)) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!AC97 Codec information";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!----------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--startcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsa-ac97.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsa-ac97.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsa-ac97-regs.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsa-ac97-regs.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--endcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if (((-s "$TEMPDIR/lsusb.tmp") > 0)) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!USB Descriptors";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!---------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--startcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/lsusb.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/lsusb.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--endcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if (((-s "$TEMPDIR/alsa-usbstream.tmp") > 0)) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!USB Stream information";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!----------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--startcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsa-usbstream.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsa-usbstream.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--endcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
if (((-s "$TEMPDIR/alsa-usbmixer.tmp") > 0)) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!USB Mixer information";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "!!---------------------";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--startcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
print (do { my $cat_chunk = q{}; if ( open my $fh, '<', $TEMPDIR ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . $TEMPDIR . ': ' . $OS_ERROR . "\n"; } $cat_chunk; } . do { my $cat_chunk = q{}; if ( open my $fh, '<', '/alsa-usbmixer.tmp' ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . '/alsa-usbmixer.tmp' . ': ' . $OS_ERROR . "\n"; } $cat_chunk; });
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "--endcollapse--";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', $FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say "";
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    }
}
my $KEEP_FILES;
if ("$1" ne q{}) {
until ( "$1" eq q{} ) {
if ("$_[0]" eq '--pastebin') {
        } elsif ("$_[0]" eq '--update') {
                        update();
            exit $main_exit_code;
        } elsif ("$_[0]" eq '--upload') {
                        $UPLOAD = 'yes';
        } elsif ("$_[0]" eq '--no-upload') {
                        $UPLOAD = 'no';
        } elsif ("$_[0]" eq '--output') {
            # Builtin command 'shift' not implemented
                        $NFILE = "$_[0]";
                        $KEEP_OUTPUT = 'yes';
        } elsif ("$_[0]" eq '--debug') {
                        say "Debugging enabled. $FILE and $TEMPDIR will not be deleted";
                        $KEEP_FILES = 'yes';
                        say "";
        } elsif ("$_[0]" eq '--with-all') {
                        withall();
        } elsif ("$_[0]" eq '--with-aplay') {
                        withaplay();
                        $WITHALL = 'no';
        } elsif ("$_[0]" eq '--with-amixer') {
                        withamixer();
                        $WITHALL = 'no';
        } elsif ("$_[0]" eq '--with-alsactl') {
                        withalsactl();
                        $WITHALL = 'no';
        } elsif ("$_[0]" eq '--with-devices') {
                        withdevices();
                        $WITHALL = 'no';
        } elsif ("$_[0]" eq '--with-dmesg') {
                        withdmesg();
                        $WITHALL = 'no';
        } elsif ("$_[0]" eq '--with-configs') {
                        withconfigs();
                        $WITHALL = 'no';
        } elsif ("$_[0]" eq '--with-packages') {
                        withpackages();
                        $WITHALL = 'no';
        } elsif ("$_[0]" eq '--stdout') {
                        $UPLOAD = 'no';
            if ("$WITHALL" eq q{}) {
                withall();
            }
            print do { my $cat_chunk = q{}; if ( open my $fh, '<', "$FILE" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$FILE" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
            if ( -e "$FILE" ) {
                if ( -d "$FILE" ) {
                    croak "rm: ", "$FILE",
          " is a directory (use -r to remove recursively)\n";
                }
                else {
                    if ( unlink "$FILE" ) {
                                            }
                    else {
                        croak "rm: cannot remove ", "$FILE",
              ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: ", "$FILE", ": No such file or directory\n";
            }
            exit $main_exit_code;
        } elsif ("$_[0]" eq '--about') {
                        say "Written/Tested by the following users of #alsa on irc.freenode.net:";
                        say "";
                        say "	wishie - Script author and developer / Testing";
                        say "	crimsun - Various script ideas / Testing";
                        say "	gnubien - Various script ideas / Testing";
                        say "	GrueMaster - HDA Intel specific items / Testing";
                        say "	olegfink - Script update function";
                        say "  TheMuso - display to stdout functionality";
            exit 0;
        } elsif (1) {
                        say "alsa-info.sh version $SCRIPT_VERSION";
                        say "";
                        say "Available options:";
                        say "	--with-aplay (includes the output of aplay -l)";
                        say "	--with-amixer (includes the output of amixer)";
                        say "	--with-alsactl (includes the output of alsactl)";
                        say "	--with-configs (includes the output of ~/.asoundrc and";
                        say "	    /etc/asound.conf if they exist)";
                        say "	--with-devices (shows the device nodes in /dev/snd/)";
                        say "	--with-dmesg (shows the ALSA/HDA kernel messages)";
                        say "	--with-packages (includes known packages installed)";
                        say "";
                        say "	--output FILE (specify the file to output for no-upload mode)";
                        say "	--update (check server for script updates)";
                        say "	--upload (upload contents to remote server)";
                        say "	--no-upload (do not upload contents to remote server)";
                        say "	--pastebin (use 'https://pastebin.ca') as remote server";
                        say "	    instead www.alsa-project.org";
                        say "	--stdout (print alsa information to standard output";
                        say "	    instead of a file)";
                        say "	--about (show some information about the script)";
                        say "	--debug (will run the script as normal, but will not";
                        say "	     delete " . ${FILE} . ")";
            exit 0;
        }
# Builtin command 'shift' not implemented
    }
;
}
if ("$PROCEED" eq no) {
exit 1;
}
if ("$WITHALL" eq q{}) {
    withall();
}
if (# Original bash: wget --help 2>/dev/null | grep -q post-file;
do {
    my $output_92 = q{};
    my $output_printed_92;
    my $pipeline_success_92 = 1;
        $output = q{};
        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_93 = q{};
use LWP::Simple;
my $url = '--help';
my $content = get($url);
if (defined $content) {
print $content;
} else {
die "Failed to download $url\n";
}
$tmp_redirect_93;
    };
    $output_92 = $output;

        my $grep_result_92_1;
    my @grep_lines_92_1 = split /\n/msx, $output_92;
    my @grep_filtered_92_1 = grep { /post-file/msx } @grep_lines_92_1;
    $grep_result_92_1 = join "\n", @grep_filtered_92_1;
    if (!($grep_result_92_1 =~ m{\n\z} || $grep_result_92_1 eq q{})) {
    $grep_result_92_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_92_1 > 0 ? 0 : 1;
    $grep_result_92_1 = q{};
    $output_92 = q{};
    if ((scalar @grep_filtered_92_1) == 0) {
        $pipeline_success_92 = 0;
    }
    if ($output_92 ne q{} && !defined $output_printed_92) {
        print $output_92;
        if (!($output_92 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_92 ) { $main_exit_code = 1; }
    }) {
if ("$UPLOAD" ne yes) {
        $main_exit_code = system('bash', ':') >> 8;
}
    else {
        if ("$DIALOG" ne q{}) {
if ("$PASTEBIN" eq q{}) {
                $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--msgbox', "Could not automatically upload output to 'https://www.alsa-project.org'.\nPossible reasons are:\n\n    1. Couldn't find 'wget' in your PATH\n    2. Your version of wget is less than 1.8.2\n\nPlease manually upload $NFILE to 'https://www.alsa-project.org/cardinfo-db' and submit your post.", '25', '100') >> 8;
}
            else {
                $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--msgbox', "Could not automatically upload output to 'https://www.pastebin.ca'.\nPossible reasons are:\n\n    1. Couldn't find 'wget' in your PATH\n    2. Your version of wget is less than 1.8.2\n\nPlease manually upload $NFILE to 'https://www.pastebin.ca/upload.php' and submit your post.", '25', '100') >> 8;
            }
}
        else {
if ("$PASTEBIN" eq q{}) {
                say "";
                say "Could not automatically upload output to 'https://www.alsa-project.org'";
                say "Possible reasons are:";
                say "    1. Couldn't find 'wget' in your PATH";
                say "    2. Your version of wget is less than 1.8.2";
                say "";
                say "Please manually upload $NFILE to 'https://www.alsa-project.org/cardinfo-db' and submit your post.";
                say "";
}
            else {
                say "";
                say "Could not automatically upload output to 'https://www.pastebin.ca'";
                say "Possible reasons are:";
                say "    1. Couldn't find 'wget' in your PATH";
                say "    2. Your version of wget is less than 1.8.2";
                say "";
                say "Please manually upload $NFILE to 'https://www.pastebin.ca/upload.php' and submit your post.";
                say "";
            }
        }
    }
    $UPLOAD = 'no';
}
if ("$UPLOAD" eq ask) {
if ("$DIALOG" ne q{}) {
        $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--title', "Information collected", '--yes-label', " UPLOAD / SHARE ", '--no-label', " SAVE LOCALLY ", '--defaultno', '--yesno', "\n\nAutomatically upload ALSA information to $WWWSERVICE?", '10', '80') >> 8;
        $DIALOG_EXIT_CODE = "${\($? >> 8)}";
if ("$DIALOG_EXIT_CODE" ne 0) {
            $UPLOAD = 'no';
}
        else {
            $UPLOAD = 'yes';
        }
}
    else {
        print "Automatically upload ALSA information to $WWWSERVICE? [y/N] : ";
$CONFIRM = <>;
chomp $CONFIRM;
$CHILD_ERROR = defined($CONFIRM) ? 0 : 1;
if ("$CONFIRM" ne y) {
            $UPLOAD = 'no';
}
        else {
            $UPLOAD = 'yes';
        }
    }
}
if ("$UPLOAD" eq no) {
        my $err;
    my $force = 1;
    if ( -e "$FILE" ) {
        my $dest = "$NFILE";
        if ( -e $dest && -d $dest ) {
            my $source_name = "$FILE";
            $source_name =~ s{^.*[\/]}{};
            $dest = "$dest/$source_name";
        }
        if ( -e $dest && !$force ) {
            croak "mv: $dest: File exists (use -f to force overwrite)\n";
        }
        my $dest_dir = $dest;
        $dest_dir =~ s/\/[^\/]*$//msx;
        if ( $dest_dir eq $dest ) {
            $dest_dir = q{};
        }
        if ( $dest_dir ne q{} && !-d $dest_dir ) {
            my $err;
            make_path( $dest_dir, { error => \$err } );
            if ( @{$err} ) {
                croak "mv: cannot create directory $dest_dir: $err->[0]\n";
            }
        }
        require File::Copy;
        if ( File::Copy::move( "$FILE", $dest ) ) {
        } else {
            croak
  "mv: cannot move "$FILE" to $dest: $ERRNO\n";
        }
    } else {
        croak "mv: "$FILE": No such file or directory\n";
    }
    if ($CHILD_ERROR != 0) {
        exit 1;
    }
;
    $KEEP_OUTPUT = 'yes';
if ("$DIALOG" ne q{}) {
        $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--title', "Information collected", '--msgbox', "\n\nYour ALSA information is in $NFILE", '10', '60') >> 8;
}
    else {
        say "";
        say "Your ALSA information is in $NFILE";
        say "";
    }
exit $main_exit_code;
}
if ("$DIALOG" ne q{}) {
    $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--infobox', "Uploading information to $WWWSERVICE ...", q{6}, '70') >> 8;
}
else {
    print "Uploading information to $WWWSERVICE ...";
}
if ("$PASTEBIN" eq q{}) {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$TEMPDIR/wget.tmp"
      or die "Cannot access file: $OS_ERROR\n";
use LWP::Simple;
my $url = 'https://www.alsa-project.org/cardinfo-db/';
my $output_file = q{-};
my $content = get($url);
if (defined $content) {
open my $fh, '>', $output_file or die "Cannot open $output_file: $ERRNO";
print {$fh} $content;
close $fh or croak "Close failed: $ERRNO";
print "Downloaded to $output_file\n";
} else {
die "Failed to download $url\n";
}
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
}
else {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$TEMPDIR/wget.tmp"
      or die "Cannot access file: $OS_ERROR\n";
use LWP::Simple;
my $url = '&encrypt=t&encryptpw=blahblah';
my $output_file = q{-};
my $content = get($url);
if (defined $content) {
open my $fh, '>', $output_file or die "Cannot open $output_file: $ERRNO";
print {$fh} $content;
close $fh or croak "Close failed: $ERRNO";
print "Downloaded to $output_file\n";
} else {
die "Failed to download $url\n";
}
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
}
if (($? != 0)) {
        if ( -e "$FILE" ) {
        my $dest = "$NFILE";
        if ( -e $dest && -d $dest ) {
            my $source_name = "$FILE";
            $source_name =~ s{^.*[\/]}{};
            $dest = "$dest/$source_name";
        }
        if ( -e $dest && !$force ) {
            croak "mv: $dest: File exists (use -f to force overwrite)\n";
        }
        my $dest_dir = $dest;
        $dest_dir =~ s/\/[^\/]*$//msx;
        if ( $dest_dir eq $dest ) {
            $dest_dir = q{};
        }
        if ( $dest_dir ne q{} && !-d $dest_dir ) {
            my $err;
            make_path( $dest_dir, { error => \$err } );
            if ( @{$err} ) {
                croak "mv: cannot create directory $dest_dir: $err->[0]\n";
            }
        }
        require File::Copy;
        if ( File::Copy::move( "$FILE", $dest ) ) {
        } else {
            croak
  "mv: cannot move "$FILE" to $dest: $ERRNO\n";
        }
    } else {
        croak "mv: "$FILE": No such file or directory\n";
    }
    if ($CHILD_ERROR != 0) {
        exit 1;
    }
    $KEEP_OUTPUT = 'yes';
if ("$DIALOG" ne q{}) {
        $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--title', "Information not uploaded", '--msgbox', "An error occurred while contacting $WWWSERVICE.\n Your information was NOT automatically uploaded.\n\nYour ALSA information is in $NFILE", '10', '100') >> 8;
}
    else {
        say "";
        say "An error occurred while contacting $WWWSERVICE.";
        say "Your information was NOT automatically uploaded.";
        say "";
        say "Your ALSA information is in $NFILE";
        say "";
    }
exit $main_exit_code;
}
if ("$DIALOG" ne q{}) {
    $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--title', "Information uploaded", '--yesno', "Would you like to see the uploaded information?", q{5}, '100') >> 8;
    $DIALOG_EXIT_CODE = "${\($? >> 8)}";
if ("$DIALOG_EXIT_CODE" eq 0) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', "$TEMPDIR/uploaded.txt"
      or die "Cannot access file: $OS_ERROR\n";
my $grep_result_100;
my @grep_lines_100 = ();
my @grep_filtered_100 = grep { !/alsa-info.txt/msx } @grep_lines_100;
$grep_result_100 = join "\n", @grep_filtered_100;
            if (!($grep_result_100 =~ m{\n\z} || $grep_result_100 eq q{})) {
                $grep_result_100 .= "\n";
            }
print $grep_result_100;
$CHILD_ERROR = scalar @grep_filtered_100 > 0 ? 0 : 1;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        $main_exit_code = system('dialog', '--backtitle', "$BGTITLE", '--textbox', "$TEMPDIR/uploaded.txt", q{0}, q{0}) >> 8;
    }
    $main_exit_code = system('bash', 'clear') >> 8;
}
else {
    say " Done!";
    say "";
}
my $FINAL_URL;
if ("$PASTEBIN" eq q{}) {
    $FINAL_URL = (do { my $result_101 = qx{bash -c q{grep SUCCESS: "$TEMPDIR/wget.tmp" | cut -d ' ' -f 2} }; chomp $result_101; $result_101; });
}
else {
    $FINAL_URL = (do { my $result_102 = qx{bash -c 'grep SUCCESS: "$TEMPDIR/wget.tmp" | sed -n "s/.*\\\\:\\\\([0-9]\\\\+\\\\).*/https:\\\\/\\\\/pastebin.ca\\\\/\\\\1/p"' }; chomp $result_102; $result_102; });
}
if ((-x "$TPUT")) {
    $FINAL_URL = (do {
    my ($in_103, $out_103);
    my $pid_103 = open3($in_103, $out_103, '>&STDERR', 'tput', 'setaf', q{1});
    close $in_103 or croak 'Close failed: $OS_ERROR';
    my $result_103 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_103> };
    close $out_103 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_103, 0;
    $result_103
});
}
say "Your ALSA information is located at $FINAL_URL";
say "Please inform the person helping you.";
say "";

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $UpdateInterval;
my $DownloadUpgradeableInterval;
my $AutoAptEnable;
my $UnattendedUpgradeInterval;
my $BackupArchiveInterval;
my $UPDATED;
my $AutocleanInterval;
my $Debdelta;
my $VERBOSE;
my $CleanInterval;


sub check_stamp {
    my ($stamp, $interval) = @_;
    $stamp = "$_[0]";
    $interval = "$_[1]";
if ("$interval" eq always) {
        $main_exit_code = system('debug_echo', "check_stamp: ignoring time stamp file, interval set to always") >> 8;
return q{0};
    }
if ("$interval" eq 0) {
        $main_exit_code = system('debug_echo', "check_stamp: interval=0") >> 8;
return q{1};
    }
if ((!-f "$stamp")) {
        $main_exit_code = system('debug_echo', "check_stamp: missing time stamp file: $stamp.") >> 8;
return q{0};
    }
    my $stamp_file = "$stamp";
    $stamp = do { my @_qx_cmd = ("date '--date=$(date -r ' Variable(\"stamp_file\", false, None) ' --iso-8601)' +%s 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if ("$?" ne "0") {
        unlink('$stamp_file');
return q{0};
    }
    my $now = do { my @_qx_cmd = ("date '--date=$(date --iso-8601)' +%s 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
if ("$?" ne "0") {
return q{0};
    }
    my $delta = eval { int($now-$stamp) } // "";
if ("${interval%s}" ne "$interval") {
        $interval = (scalar reverse( (scalar reverse ${interval}) =~ s/^s//r ) =~ s/s$//r);
}
    else {
        if ("${interval%m}" ne "$interval") {
            $interval = (scalar reverse( (scalar reverse ${interval}) =~ s/^m//r ) =~ s/m$//r);
            $interval = eval { int($interval*60) } // "";
}
        else {
            if ("${interval%h}" ne "$interval") {
                $interval = (scalar reverse( (scalar reverse ${interval}) =~ s/^h//r ) =~ s/h$//r);
                $interval = eval { int($interval*60*60) } // "";
}
            else {
                $interval = (scalar reverse( (scalar reverse ${interval}) =~ s/^d//r ) =~ s/d$//r);
                $interval = eval { int($interval*60*60*24) } // "";
            }
        }
    }
    $main_exit_code = system('debug_echo', "check_stamp: interval=$interval, now=$now, stamp=$stamp, delta=$delta (sec)") >> 8;
if (($stamp > (eval { int($now+86400) } // ""))) {
        say "WARNING: file $stamp_file has a timestamp in the future: $stamp";
        unlink('$stamp_file');
return q{0};
    }
if (($delta >= $interval)) {
return q{0};
    }
return q{1};
    return;
}

sub update_stamp {
    my ($stamp) = @_;
    $stamp = "$_[0]";
    if ( -e "$stamp" ) {
        my $current_time = time;
        utime $current_time, $current_time, "$stamp";
    }
    else {
        if ( open my $fh, '>', "$stamp" ) {
            close $fh or croak "Close failed: $ERRNO";
        }
        else {
            croak "touch: cannot create ", "$stamp",
              ": $ERRNO\n";
        }
    }
    return;
}

sub check_size_constraints {
    my $MaxAge = q{0};
do { my $eval_input = do {
    my ($in_1, $out_1);
    my $pid_1 = open3($in_1, $out_1, '>&STDERR', 'apt-config', 'shell', 'MaxAge', 'APT::Archives::MaxAge');
    close $in_1 or croak 'Close failed: $OS_ERROR';
    my $result_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_1> };
    close $out_1 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_1, 0;
    $result_1
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
do { my $eval_input = do {
    my ($in_2, $out_2);
    my $pid_2 = open3($in_2, $out_2, '>&STDERR', 'apt-config', 'shell', 'MaxAge', 'APT::Periodic::MaxAge');
    close $in_2 or croak 'Close failed: $OS_ERROR';
    my $result_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_2> };
    close $out_2 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_2, 0;
    $result_2
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
    my $MinAge = q{2};
do { my $eval_input = do {
    my ($in_3, $out_3);
    my $pid_3 = open3($in_3, $out_3, '>&STDERR', 'apt-config', 'shell', 'MinAge', 'APT::Archives::MinAge');
    close $in_3 or croak 'Close failed: $OS_ERROR';
    my $result_3 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_3> };
    close $out_3 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_3, 0;
    $result_3
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
do { my $eval_input = do {
    my ($in_4, $out_4);
    my $pid_4 = open3($in_4, $out_4, '>&STDERR', 'apt-config', 'shell', 'MinAge', 'APT::Periodic::MinAge');
    close $in_4 or croak 'Close failed: $OS_ERROR';
    my $result_4 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_4> };
    close $out_4 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_4, 0;
    $result_4
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
    my $MaxSize = q{0};
do { my $eval_input = do {
    my ($in_5, $out_5);
    my $pid_5 = open3($in_5, $out_5, '>&STDERR', 'apt-config', 'shell', 'MaxSize', 'APT::Archives::MaxSize');
    close $in_5 or croak 'Close failed: $OS_ERROR';
    my $result_5 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_5> };
    close $out_5 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_5, 0;
    $result_5
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
do { my $eval_input = do {
    my ($in_6, $out_6);
    my $pid_6 = open3($in_6, $out_6, '>&STDERR', 'apt-config', 'shell', 'MaxSize', 'APT::Periodic::MaxSize');
    close $in_6 or croak 'Close failed: $OS_ERROR';
    my $result_6 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_6> };
    close $out_6 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_6, 0;
    $result_6
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
    my $Cache = "/var/cache/apt/archives/";
do { my $eval_input = do {
    my ($in_7, $out_7);
    my $pid_7 = open3($in_7, $out_7, '>&STDERR', 'apt-config', 'shell', 'Cache', 'Dir::Cache::archives/d');
    close $in_7 or croak 'Close failed: $OS_ERROR';
    my $result_7 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_7> };
    close $out_7 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_7, 0;
    $result_7
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
if ("$Cache" eq q{}) {
        say "empty Dir::Cache::archives, exiting";
exit $main_exit_code;
    }
if (((!$MaxAge == 0) && (!$MinAge == 0))) {
        $main_exit_code = system('debug_echo', "aged: ctime <$MaxAge and mtime <$MaxAge and ctime>$MinAge and mtime>$MinAge") >> 8;
        # Original bash: find $Cache -name "*.deb"  \( -mtime +$MaxAge -and -ctime +$MaxAge \) -and -not \( -mtime -$MinAge -or -ctime -$MinAge \) -print0 | xargs -r -0 rm -f
do {
            my $output_8 = q{};
            my $output_printed_8;
            my $pipeline_success_8 = 1;
                        $output_8 = do {
            require File::Find;
            my @find_results;
            File::Find::find(sub { if ($_ =~ /^.*\.deb$/) { push @find_results, $File::Find::name; } }, "\\(");
            my $result = join "\n", @find_results;
            if ($result ne q{}) { $result .= "\n"; }
            $CHILD_ERROR = 0;
            $result;
            };

                        my @xargs_input_8_1 = grep { $_ ne q{} } split /\s+/, $output_8;
            my @xargs_output_8_1;
            for my $i (0..scalar @xargs_input_8_1-1) {
            my @xargs_args_8_1;
            for my $j (0..1-1) {
            push @xargs_args_8_1, $xargs_input_8_1[$i + $j];
            }
            my ($in_8_1, $out_8_1, $err_8_1);
            my $cmd_xargs_8_1 = 'rm';
            my $pid_8_1 = open3($in_8_1, $out_8_1, $err_8_1, $cmd_xargs_8_1, '-f', @xargs_args_8_1);
            close $in_8_1 or croak 'Close failed: $OS_ERROR';
            my $xargs_result_8_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_8_1> };
            close $out_8_1 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_8_1, 0;
            chomp $xargs_result_8_1;
            push @xargs_output_8_1, $xargs_result_8_1;
            }
            my $xargs_result_8_1 = join "\n", @xargs_output_8_1;
            if ($xargs_result_8_1 ne q{} && !( $xargs_result_8_1 =~ m{\n\z} )) { $xargs_result_8_1 .= "\n"; }
            $output_8 = $xargs_result_8_1;
            $output_8 = $xargs_result_8_1;
            if ($output_8 ne q{} && !defined $output_printed_8) {
                print $output_8;
                if (!($output_8 =~ m{\n\z})) {
                    print "\n";
                }
            }
            if ( !$pipeline_success_8 ) { $main_exit_code = 1; }
            }
;
}
    else {
        if ((!$MaxAge == 0)) {
            $main_exit_code = system('debug_echo', "aged: ctime <$MaxAge and mtime <$MaxAge only") >> 8;
            # Original bash: find $Cache -name "*.deb"  -ctime +$MaxAge -and -mtime +$MaxAge -print0 | xargs -r -0 rm -f
do {
                my $output_9 = q{};
                my $output_printed_9;
                my $pipeline_success_9 = 1;
                                $output_9 = do {
                require File::Find;
                my @find_results;
                File::Find::find(sub { if ($_ =~ /^.*\.deb$/) { push @find_results, $File::Find::name; } }, 'time');
                my $result = join "\n", @find_results;
                if ($result ne q{}) { $result .= "\n"; }
                $CHILD_ERROR = 0;
                $result;
                };

                                my @xargs_input_9_1 = grep { $_ ne q{} } split /\s+/, $output_9;
                my @xargs_output_9_1;
                for my $i (0..scalar @xargs_input_9_1-1) {
                my @xargs_args_9_1;
                for my $j (0..1-1) {
                push @xargs_args_9_1, $xargs_input_9_1[$i + $j];
                }
                my ($in_9_1, $out_9_1, $err_9_1);
                my $cmd_xargs_9_1 = 'rm';
                my $pid_9_1 = open3($in_9_1, $out_9_1, $err_9_1, $cmd_xargs_9_1, '-f', @xargs_args_9_1);
                close $in_9_1 or croak 'Close failed: $OS_ERROR';
                my $xargs_result_9_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_9_1> };
                close $out_9_1 or croak 'Close failed: $OS_ERROR';
                waitpid $pid_9_1, 0;
                chomp $xargs_result_9_1;
                push @xargs_output_9_1, $xargs_result_9_1;
                }
                my $xargs_result_9_1 = join "\n", @xargs_output_9_1;
                if ($xargs_result_9_1 ne q{} && !( $xargs_result_9_1 =~ m{\n\z} )) { $xargs_result_9_1 .= "\n"; }
                $output_9 = $xargs_result_9_1;
                $output_9 = $xargs_result_9_1;
                if ($output_9 ne q{} && !defined $output_printed_9) {
                    print $output_9;
                    if (!($output_9 =~ m{\n\z})) {
                        print "\n";
                    }
                }
                if ( !$pipeline_success_9 ) { $main_exit_code = 1; }
                }
;
}
        else {
            $main_exit_code = system('debug_echo', "skip aging since MaxAge is 0") >> 8;
        }
    }
    my $ctime;
    my $du;
    my $mtime;
    my $now;
    my $delta;
    my $size;
if ((!$MaxSize == 0)) {
        $MaxSize = eval { int($MaxSize*1024) } // "";
        $now = do {
require POSIX; POSIX::strftime('--date=$(date --iso-8601)', localtime())
};
        $MinAge = eval { int($MinAge*24*60*60) } // "";
        my $file;
        for my $file (do { my @_qx_cmd = ("ls -rt Variable(\"Cache\", false, None) '/*.deb' 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; }) {
            $du = do {
    my ($in_10, $out_10);
    my $pid_10 = open3($in_10, $out_10, '>&STDERR', 'du', '-s', $Cache);
    close $in_10 or croak 'Close failed: $OS_ERROR';
    my $result_10 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_10> };
    close $out_10 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_10, 0;
    $result_10
};
            $size = dirname(($ENV{du%} // q{}));
if (($size < $MaxSize)) {
                $main_exit_code = system('debug_echo', "end remove by archive size:  size=$size < $MaxSize") >> 8;
last;
            }
if (($MinAge != 0)) {
                $mtime = do {
    my ($in_11, $out_11);
    my $pid_11 = open3($in_11, $out_11, '>&STDERR', 'stat', '-c', '%Y', "$file");
    close $in_11 or croak 'Close failed: $OS_ERROR';
    my $result_11 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_11> };
    close $out_11 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_11, 0;
    $result_11
};
                $ctime = do {
    my ($in_12, $out_12);
    my $pid_12 = open3($in_12, $out_12, '>&STDERR', 'stat', '-c', '%Z', "$file");
    close $in_12 or croak 'Close failed: $OS_ERROR';
    my $result_12 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_12> };
    close $out_12 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_12, 0;
    $result_12
};
if (($mtime > $ctime)) {
                    $delta = eval { int($now-$mtime) } // "";
}
                else {
                    $delta = eval { int($now-$ctime) } // "";
                }
if (($delta <= $MinAge)) {
                    $main_exit_code = system('debug_echo', "skip remove by archive size:  $file, delta=$delta < $MinAge") >> 8;
last;
}
                else {
                    $main_exit_code = system('debug_echo', "remove by archive size: $file, delta=$delta >= $MinAge (sec), size=$size >= $MaxSize") >> 8;
                    unlink('$file');
                }
            }
        }
;
    }
;
    return;
}

sub do_cache_backup {
    my ($BackupArchiveInterval) = @_;
    $BackupArchiveInterval = "$_[0]";
if ("$BackupArchiveInterval" eq always) {
        $main_exit_code = system('bash', ':') >> 8;
}
    else {
        if ("$BackupArchiveInterval" eq 0) {
return;
        }
    }
    my $CacheDir = "/var/cache/apt";
do { my $eval_input = do {
    my ($in_13, $out_13);
    my $pid_13 = open3($in_13, $out_13, '>&STDERR', 'apt-config', 'shell', 'CacheDir', 'Dir::Cache/d');
    close $in_13 or croak 'Close failed: $OS_ERROR';
    my $result_13 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_13> };
    close $out_13 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_13, 0;
    $result_13
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
    $CacheDir = scalar reverse( (scalar reverse ${CacheDir}) =~ s/^///r );
if ("$CacheDir" eq q{}) {
        $main_exit_code = system('debug_echo', "practically empty Dir::Cache, exiting") >> 8;
return q{0};
    }
    my $Cache = ${CacheDir} . "/archives/";
do { my $eval_input = do {
    my ($in_14, $out_14);
    my $pid_14 = open3($in_14, $out_14, '>&STDERR', 'apt-config', 'shell', 'Cache', 'Dir::Cache::Archives/d');
    close $in_14 or croak 'Close failed: $OS_ERROR';
    my $result_14 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_14> };
    close $out_14 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_14, 0;
    $result_14
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
if ("$Cache" eq q{}) {
        $main_exit_code = system('debug_echo', "practically empty Dir::Cache::archives, exiting") >> 8;
return q{0};
    }
    my $BackupLevel = q{3};
do { my $eval_input = do {
    my ($in_15, $out_15);
    my $pid_15 = open3($in_15, $out_15, '>&STDERR', 'apt-config', 'shell', 'BackupLevel', 'APT::Periodic::BackupLevel');
    close $in_15 or croak 'Close failed: $OS_ERROR';
    my $result_15 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_15> };
    close $out_15 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_15, 0;
    $result_15
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
if (($BackupLevel <= 1)) {
        $BackupLevel = q{2};
    }
    my $Back = ${CacheDir} . "/backup/";
do { my $eval_input = do {
    my ($in_16, $out_16);
    my $pid_16 = open3($in_16, $out_16, '>&STDERR', 'apt-config', 'shell', 'Back', 'Dir::Cache::Backup/d');
    close $in_16 or croak 'Close failed: $OS_ERROR';
    my $result_16 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_16> };
    close $out_16 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_16, 0;
    $result_16
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
if ("$Back" eq q{}) {
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
            say "practically empty Dir::Cache::Backup, exiting";
        };
return;
    }
    my $CacheArchive = (do { use File::Basename qw(basename); my $basename_output = basename(${Cache}); $CHILD_ERROR = 0; $basename_output; });
        $main_exit_code = system('test', '-n', ${CacheArchive}) >> 8;
    if ($CHILD_ERROR != 0) {
                $CacheArchive = "archives";
    }
;
    my $BackX = ${Back} . ${CacheArchive} . "/";
    my $x;
    for my $x (do { my $first; my $increment; my $last; my @result; my $i; $first = q{0}; $increment = q{1}; $last = eval { int($BackupLevel-1) } // ""; for ($i = $first; $i <= $last; $i += $increment) { push @result, $i; } join "\n", @result; }) {
do { my $eval_input = "Back" . ${x} . "=" . ${Back} . ${x} . "/"; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
    }
;
    my $BACKUP_ARCHIVE_STAMP = '/var/lib/apt/periodic/backup-archive-stamp';
if (!(    check_stamp(stamp => $BACKUP_ARCHIVE_STAMP, interval => "$BackupArchiveInterval"))) {
if ((qx'{ (cd $Cache 2>/dev/null; find . -name "*.deb"); (cd $Back0 2>/dev/null;find . -name "*.deb") ;}| sort|uniq -u|wc -l' != 0)) {
            use File::Path qw(make_path);
            my $err;
            if ( !-d $Back ) {
                make_path( $Back, { error => \$err } );
                if ( @{$err} ) {
                    croak "mkdir: cannot create directory " . $Back . ": $err->[0]\n";
                }
            }
;
if ( -e "$Back" ) {
                if ( -d "$Back" ) {
                    my $err;
                    require File::Path;
                    File::Path::remove_tree("$Back", {error => \$err});
                    if (@{$err}) {
                        carp "rm: carping: could not remove ", $Back, ": $err->[0]\n";
                    }
                    else {
                                            }
                }
                else {
                    if ( unlink "$Back" ) {
                                            }
                    else {
                        carp "rm: carping: could not remove ", $Back,
              ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 0;
            }
if ( -e "eval { int($BackupLevel-1) } // """ ) {
                if ( -d "eval { int($BackupLevel-1) } // """ ) {
                    my $err;
                    require File::Path;
                    File::Path::remove_tree("eval { int($BackupLevel-1) } // """, {error => \$err});
                    if (@{$err}) {
                        carp "rm: carping: could not remove ", eval { int($BackupLevel-1) } // "", ": $err->[0]\n";
                    }
                    else {
                                            }
                }
                else {
                    if ( unlink "eval { int($BackupLevel-1) } // """ ) {
                                            }
                    else {
                        carp "rm: carping: could not remove ", eval { int($BackupLevel-1) } // "",
              ": $OS_ERROR\n";
                    }
                }
            }
            else {
                local $CHILD_ERROR = 0;
            }
            my $y;
            for my $y (do { my $first; my $increment; my $last; my @result; my $i; $first = eval { int($BackupLevel-1) } // ""; $increment = '-1'; $last = q{1}; for ($i = $first; $i <= $last; $i += $increment) { push @result, $i; } join "\n", @result; }) {
do { my $eval_input = "BackY" . "=" . $Back . $y; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
do { my $eval_input = "BackZ" . "=" . $Back . eval { int($y-1) } // ""; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
if ((-e $BackZ)) {
                    my $force = 1;
                    if ( -e "$BackZ" ) {
                        my $dest = $BackY;
                        if ( -e $dest && -d $dest ) {
                            my $source_name = "$BackZ";
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
                        if ( File::Copy::move( "$BackZ", $dest ) ) {
                        } else {
                            croak
  "mv: cannot move "$BackZ" to $dest: $ERRNO\n";
                        }
                    } else {
                        croak "mv: "$BackZ": No such file or directory\n";
                    }
;
                }
            }
;
            use File::Copy qw(copy);
            if ( -e $Cache ) {
                if ( -d $Back ) {
                    require File::Copy; File::Copy::copy($Cache, $Back . '/' . ($Cache =~ m|([^/]+)$|)[0]);
                } else {
                    require File::Copy; File::Copy::copy($Cache, $Back);
                }
            } else {
                croak "cp: cannot stat '-la': No such file or directory\n";
            }
;
            if ( -e "$BackX" ) {
                my $dest = $Back0;
                if ( -e $dest && -d $dest ) {
                    my $source_name = "$BackX";
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
                if ( File::Copy::move( "$BackX", $dest ) ) {
                } else {
                    croak
  "mv: cannot move "$BackX" to $dest: $ERRNO\n";
                }
            } else {
                croak "mv: "$BackX": No such file or directory\n";
            }
            update_stamp(stamp => $BACKUP_ARCHIVE_STAMP);
            $main_exit_code = system('debug_echo', "backup with hardlinks. (success)") >> 8;
}
        else {
            $main_exit_code = system('debug_echo', "skip backup since same content.") >> 8;
        }
}
    else {
        $main_exit_code = system('debug_echo', "skip backup since too new.") >> 8;
    }
    return;
}

sub debug_echo {
    my ($file) = @_;
if (($VERBOSE >= 1)) {
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
            say $_[0];
        };
    }
    return;
}
if ("$1" eq "lock_is_held") {
# Builtin command 'shift' not implemented
}
else {
do { my $eval_input = do {
    my ($in_21, $out_21);
    my $pid_21 = open3($in_21, $out_21, '>&STDERR', 'apt-config', 'shell', 'StateDir', 'Dir::State/d');
    close $in_21 or croak 'Close failed: $OS_ERROR';
    my $result_21 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_21> };
    close $out_21 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_21, 0;
    $result_21
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', $StateDir
      or die "Cannot access file: $OS_ERROR\n";
# Builtin command 'exec' not implemented
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    $main_exit_code = system('bash', '/daily_lock') >> 8;
if (system('flock', '-w', '3600', q{3}) >> 8) {
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
            say "E: Could not acquire lock";
        };
exit 1;
    }
    do {
local *STDERR;
open STDERR, '>', q{-} or croak "Cannot access file: $OS_ERROR\n";
        $CHILD_ERROR = 0;
    };
}
if ((-r '/var/lib/apt/extended_states')) {
if (!(    chdir('/var/backups');
    $CHILD_ERROR = 0)) {
if (system('cmp', '-s', 'apt.extended_states.0', '/var/lib/apt/extended_states') >> 8) {
            use File::Copy qw(copy);
            if ( -e '/var/lib/apt/extended_states' ) {
                if ( -d 'apt.extended_states' ) {
                    require File::Copy; File::Copy::copy('/var/lib/apt/extended_states', 'apt.extended_states' . '/' . ('/var/lib/apt/extended_states' =~ m|([^/]+)$|)[0]);
                } else {
                    require File::Copy; File::Copy::copy('/var/lib/apt/extended_states', 'apt.extended_states');
                }
            } else {
                croak "cp: cannot stat '-p': No such file or directory\n";
            }
;
            do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
                my $tmp = do {
                $main_exit_code = system('savelog', '-c', q{7}, 'apt.extended_states') >> 8;
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
if (do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    $main_exit_code = system('command', '-v', 'apt-config') >> 8;
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
}) {
exit 0;
}
$AutoAptEnable = q{1};
do { my $eval_input = do {
    my ($in_23, $out_23);
    my $pid_23 = open3($in_23, $out_23, '>&STDERR', 'apt-config', 'shell', 'AutoAptEnable', 'APT::Periodic::Enable');
    close $in_23 or croak 'Close failed: $OS_ERROR';
    my $result_23 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_23> };
    close $out_23 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_23, 0;
    $result_23
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
if (($AutoAptEnable == 0)) {
exit 0;
}
$VERBOSE = q{0};
do { my $eval_input = do {
    my ($in_24, $out_24);
    my $pid_24 = open3($in_24, $out_24, '>&STDERR', 'apt-config', 'shell', 'VERBOSE', 'APT::Periodic::Verbose');
    close $in_24 or croak 'Close failed: $OS_ERROR';
    my $result_24 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_24> };
    close $out_24 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_24, 0;
    $result_24
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
debug_echo("verbose level $VERBOSE");
my $XAPTOPT;
my $XSTDERR;
my $XSTDOUT;
my $XUUPOPT;
if (($VERBOSE <= 1)) {
    $XSTDOUT = ">/dev/null";
    $XSTDERR = "2>/dev/null";
    $XAPTOPT = "-qq";
    $XUUPOPT = "";
}
else {
    $XSTDOUT = "";
    $XSTDERR = "";
    $XAPTOPT = "";
    $XUUPOPT = "-d";
}
if (($VERBOSE >= 3)) {
# set -x not implemented
}
if ((!(do {
    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    $main_exit_code = system('command', '-v', 'apt-get') >> 8;
    };
    print $tmp;
    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
}) && !(!(do { my $eval_input = "apt-get" . "check" . $XAPTOPT . $XSTDERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)))) {
    debug_echo("error encountered in cron job with \"apt-get check\".");
exit 0;
}
my $now = do {
require POSIX; POSIX::strftime('%s', localtime())
};
$UpdateInterval = q{0};
do { my $eval_input = do {
    my ($in_25, $out_25);
    my $pid_25 = open3($in_25, $out_25, '>&STDERR', 'apt-config', 'shell', 'UpdateInterval', 'APT::Periodic::Update-Package-Lists');
    close $in_25 or croak 'Close failed: $OS_ERROR';
    my $result_25 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_25> };
    close $out_25 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_25, 0;
    $result_25
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
$DownloadUpgradeableInterval = q{0};
do { my $eval_input = do {
    my ($in_26, $out_26);
    my $pid_26 = open3($in_26, $out_26, '>&STDERR', 'apt-config', 'shell', 'DownloadUpgradeableInterval', 'APT::Periodic::Download-Upgradeable-Packages');
    close $in_26 or croak 'Close failed: $OS_ERROR';
    my $result_26 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_26> };
    close $out_26 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_26, 0;
    $result_26
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
$UnattendedUpgradeInterval = q{0};
do { my $eval_input = do {
    my ($in_27, $out_27);
    my $pid_27 = open3($in_27, $out_27, '>&STDERR', 'apt-config', 'shell', 'UnattendedUpgradeInterval', 'APT::Periodic::Unattended-Upgrade');
    close $in_27 or croak 'Close failed: $OS_ERROR';
    my $result_27 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_27> };
    close $out_27 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_27, 0;
    $result_27
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
$AutocleanInterval = q{0};
do { my $eval_input = do {
    my ($in_28, $out_28);
    my $pid_28 = open3($in_28, $out_28, '>&STDERR', 'apt-config', 'shell', 'AutocleanInterval', 'APT::Periodic::AutocleanInterval');
    close $in_28 or croak 'Close failed: $OS_ERROR';
    my $result_28 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_28> };
    close $out_28 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_28, 0;
    $result_28
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
$CleanInterval = q{0};
do { my $eval_input = do {
    my ($in_29, $out_29);
    my $pid_29 = open3($in_29, $out_29, '>&STDERR', 'apt-config', 'shell', 'CleanInterval', 'APT::Periodic::CleanInterval');
    close $in_29 or croak 'Close failed: $OS_ERROR';
    my $result_29 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_29> };
    close $out_29 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_29, 0;
    $result_29
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
$BackupArchiveInterval = q{0};
do { my $eval_input = do {
    my ($in_30, $out_30);
    my $pid_30 = open3($in_30, $out_30, '>&STDERR', 'apt-config', 'shell', 'BackupArchiveInterval', 'APT::Periodic::BackupArchiveInterval');
    close $in_30 or croak 'Close failed: $OS_ERROR';
    my $result_30 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_30> };
    close $out_30 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_30, 0;
    $result_30
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
$Debdelta = q{1};
do { my $eval_input = do {
    my ($in_31, $out_31);
    my $pid_31 = open3($in_31, $out_31, '>&STDERR', 'apt-config', 'shell', 'Debdelta', 'APT::Periodic::Download-Upgradeable-Packages-Debdelta');
    close $in_31 or croak 'Close failed: $OS_ERROR';
    my $result_31 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_31> };
    close $out_31 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_31, 0;
    $result_31
}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
if (((((($UpdateInterval eq always || $DownloadUpgradeableInterval eq always) || $UnattendedUpgradeInterval eq always) || $BackupArchiveInterval eq always) || $AutocleanInterval eq always) || $CleanInterval eq always)) {
    $main_exit_code = system('bash', ':') >> 8;
}
else {
    if (((((($UpdateInterval eq 0 && $DownloadUpgradeableInterval eq 0) && $UnattendedUpgradeInterval eq 0) && $BackupArchiveInterval eq 0) && $AutocleanInterval eq 0) && $CleanInterval eq 0)) {
        check_size_constraints();
exit 0;
    }
}
my $UPDATE_STAMP;
my $DOWNLOAD_UPGRADEABLE_STAMP;
if (("$1" eq "update" || "$1" eq q{})) {
    do_cache_backup(BackupArchiveInterval => $BackupArchiveInterval);
if ((-r '/etc/default/locale')) {
        $main_exit_code = system('.', '/etc/default/locale') >> 8;
$ENV{LANG} = $LANG;
$ENV{LANGUAGE} = $LANGUAGE;
$ENV{LC_MESSAGES} = $LC_MESSAGES;
$ENV{LC_ALL} = $LC_ALL;
    }
    $UPDATED = q{0};
    $UPDATE_STAMP = '/var/lib/apt/periodic/update-stamp';
if (!(    check_stamp(stamp => $UPDATE_STAMP, interval => $UpdateInterval))) {
if (!(do { my $eval_input = "apt-get" . $XAPTOPT . "-y" . "update" . $XSTDERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
            debug_echo("download updated metadata (success).");
            update_stamp(stamp => $UPDATE_STAMP);
            $UPDATED = q{1};
}
        else {
            debug_echo("download updated metadata (error)");
        }
}
    else {
        debug_echo("download updated metadata (not run).");
    }
    $DOWNLOAD_UPGRADEABLE_STAMP = '/var/lib/apt/periodic/download-upgradeable-stamp';
if ((($UPDATED == 1) && !(    check_stamp(stamp => $DOWNLOAD_UPGRADEABLE_STAMP, interval => $DownloadUpgradeableInterval)))) {
if (($Debdelta == 1)) {
                        do {
                open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
                my $tmp = do {
                $main_exit_code = system('bash', 'debdelta-upgrade') >> 8;
                };
                print $tmp;
                open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                close $original_stdout
      or die "Close failed: $OS_ERROR\n";
            };
            if ($CHILD_ERROR != 0) {
                1;
            }
;
        }
if (!(do { my $eval_input = "apt-get" . $XAPTOPT . "-y" . "-d" . "dist-upgrade" . $XSTDERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
            update_stamp(stamp => $DOWNLOAD_UPGRADEABLE_STAMP);
            debug_echo("download upgradable (success)");
}
        else {
            debug_echo("download upgradable (error)");
        }
}
    else {
        debug_echo("download upgradable (not run)");
    }
if (((!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', 'unattended-upgrade') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    }) && !(do {
        my $output_33 = q{};
        my $output_printed_33;
        my $pipeline_success_33 = 1;
                my ($in_34, $out_34);
        my $pid_34 = open3($in_34, $out_34, '>&STDERR', 'env', 'LC_ALL', q{=}, 'C.UTF-8', 'unattended-upgrade', '--help');
        close $in_34 or croak 'Close failed: $OS_ERROR';
        $output_33 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_34> };
        close $out_34 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_34, 0;

                my $grep_result_33_1;
        my @grep_lines_33_1 = split /\n/msx, $output_33;
        my @grep_filtered_33_1 = grep { /download-only/msx } @grep_lines_33_1;
        $grep_result_33_1 = join "\n", @grep_filtered_33_1;
        if (!($grep_result_33_1 =~ m{\n\z} || $grep_result_33_1 eq q{})) {
        $grep_result_33_1 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_33_1 > 0 ? 0 : 1;
        $grep_result_33_1 = q{};
        $output_33 = q{};
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
        })) && !(    check_stamp(stamp => $DOWNLOAD_UPGRADEABLE_STAMP, interval => $UnattendedUpgradeInterval)))) {
if (!(system('unattended-upgrade', '--download-only', $XUUPOPT) >> 8)) {
            update_stamp(stamp => $DOWNLOAD_UPGRADEABLE_STAMP);
            debug_echo("unattended-upgrade -d (success)");
}
        else {
            debug_echo("unattended-upgrade -d (error)");
        }
}
    else {
        debug_echo("unattended-upgrade -d (not run)");
    }
}
my $CLEAN_STAMP;
my $UPGRADE_STAMP;
my $AUTOCLEAN_STAMP;
if (("$1" eq "install" || "$1" eq q{})) {
    $UPGRADE_STAMP = '/var/lib/apt/periodic/upgrade-stamp';
if ((!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', 'unattended-upgrade') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    }) && !(    check_stamp(stamp => $UPGRADE_STAMP, interval => $UnattendedUpgradeInterval)))) {
if (!(system('unattended-upgrade', $XUUPOPT) >> 8)) {
            update_stamp(stamp => $UPGRADE_STAMP);
            debug_echo("unattended-upgrade (success)");
}
        else {
            debug_echo("unattended-upgrade (error)");
        }
}
    else {
        debug_echo("unattended-upgrade (not run)");
    }
    $CLEAN_STAMP = '/var/lib/apt/periodic/clean-stamp';
if (!(    check_stamp(stamp => $CLEAN_STAMP, interval => $CleanInterval))) {
if (!(do { my $eval_input = "apt-get" . $XAPTOPT . "-y" . "clean" . $XSTDERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
            debug_echo("clean (success).");
            update_stamp(stamp => $CLEAN_STAMP);
}
        else {
            debug_echo("clean (error)");
        }
}
    else {
        debug_echo("clean (not run)");
    }
    $AUTOCLEAN_STAMP = '/var/lib/apt/periodic/autoclean-stamp';
if (!(    check_stamp(stamp => $AUTOCLEAN_STAMP, interval => $AutocleanInterval))) {
if (!(do { my $eval_input = "apt-get" . $XAPTOPT . "-y" . "autoclean" . $XSTDERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
            debug_echo("autoclean (success).");
            update_stamp(stamp => $AUTOCLEAN_STAMP);
}
        else {
            debug_echo("autoclean (error)");
        }
}
    else {
        debug_echo("autoclean (not run)");
    }
    check_size_constraints();
}

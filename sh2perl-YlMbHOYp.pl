#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $cur;
my $next;
my $SGDISK;
my $SFDISK;
my $DISK;
my $PART;
my $RESIZE_RESULT;

my $FUDGE = (defined ($ENV{GROWPART_FUDGE} // q{}) && ($ENV{GROWPART_FUDGE} // q{}) ne q{} ? ($ENV{GROWPART_FUDGE} // q{}) : do { my $_result = do {
    my ($in_0, $out_0);
    my $pid_0 = open3($in_0, $out_0, '>&STDERR', '1024*1024');
    close $in_0 or croak 'Close failed: $OS_ERROR';
    my $result_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_0> };
    close $out_0 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_0, 0;
    $result_0
}; $_result; });
my $TEMP_D = "";
my $RESTORE_FUNC = "";
my $RESTORE_HUMAN = "";
my $VERBOSITY = q{0};
$DISK = "";
$PART = "";
my $PT_UPDATE = 'false';
my $DRY_RUN = q{0};
my $FLOCK_DISK_FD = "";
$RESIZE_RESULT = "";
my $SFDISK_VERSION = "";
my $SFDISK_2_26 = "22600";
my $SFDISK_V_WORKING_GPT = "22603";
my $MBR_BACKUP = "";
my $GPT_BACKUP = "";
my $_capture = "";

sub error {
    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        say @ARGV;
    };
    return;
}

sub fail {
    if (!((scalar(@ARGV) == 0))) {
                say "FAILED:" . q{ } . @ARGV;
    }
exit 2;
    return;
}

sub nochange {
    $RESIZE_RESULT = "NOCHANGE";
    say "NOCHANGE:" . q{ } . @ARGV;
return q{1};
    return;
}

sub changed {
    $RESIZE_RESULT = "CHANGED";
    say "CHANGED:" . q{ } . @ARGV;
return q{0};
    return;
}

sub change {
    $RESIZE_RESULT = "CHANGE";
    say "CHANGE:" . q{ } . @ARGV;
return q{0};
    return;
}

sub cleanup {
if ("${RESTORE_FUNC}" ne q{}) {
        error("***** WARNING: Resize failed, attempting to revert ******");
if (!(        $CHILD_ERROR = 0)) {
            error("***** Restore appears to have gone OK ****");
}
        else {
            error("***** Restore FAILED! ******");
if (("${RESTORE_HUMAN}" ne q{} && (-f "${RESTORE_HUMAN}"))) {
                error("**** original table looked like: ****");
                do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', ${RESTORE_HUMAN} ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . ${RESTORE_HUMAN} . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                };
}
            else {
                error("We seem to have not saved the partition table!");
            }
        }
        $main_exit_code = system('unlock_disk_and_settle', $DISK) >> 8;
    }
    if (!(("${TEMP_D}" eq q{} || (!-d "${TEMP_D}")))) {
        do { my $rm_cmd_str = 'rm -Rf "${TEMP_D}"'; system $rm_cmd_str; };
    }
    return;
}

sub debug {
    my ($file) = @_;
    my $level = $_[0];
# Builtin command 'shift' not implemented
    if ((${level} > ${VERBOSITY})) {
        return;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
if (("${DEBUG_LOG}")) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', ($ENV{DEBUG_LOG} // q{})
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            say @ARGV;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
}
    else {
        error("\@ARGV");
    }
    return;
}

sub debugcat {
    my ($file) = @_;
    my $level = "$_[0]";
# Builtin command 'shift' not implemented
    if ((${level} > $VERBOSITY)) {
        return;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
if (("${DEBUG_LOG}")) {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>>', ($ENV{DEBUG_LOG} // q{})
      or die "Cannot access file: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "\@ARGV" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "\@ARGV" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
}
    else {
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "\@ARGV" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "\@ARGV" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
        };
    }
    return;
}

sub mktemp_d {
    if (do {
my $_RET = do { my @_qx_cmd = ("mktemp -d \"${TMPDIR}/${0}.XXXXXX\" 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $CHILD_ERROR == 0
    }) {
        return;
    }
    $_RET = do { my @_qx_cmd = ('umask 077 && t="${TMPDIR}/${0}.$$" && mkdir "${t}" && echo "${t}"'); my $result = qx{$_qx_cmd[0]}; $CHILD_ERROR = $? >> 8; $result; };
return;
    return;
}

sub Usage {
print "${0##*/} disk partition
   rewrite partition table so that partition takes up all the space it can
   options:
    -h | --help            print Usage and exit
         --free-percent F  resize so that specified percentage F of the disk is
                           not used in total (not just by this partition). This
                           is useful for consumer SSD or SD cards where a small
                           percentage unallocated can improve device lifetime.
         --fudge F         if part could be resized, but change would be less
                           than 'F' bytes, do not resize (default: ${FUDGE})
    -N | --dry-run         only report what would be done, show new 'sfdisk -d'
    -v | --verbose         increase verbosity / debug
    -u | --update  R       update the the kernel partition table info after
                           growing this requires kernel support and
                           'partx --update'
                           R is one of:
                            - 'auto'  : [default] update partition if possible
                            - 'force' : try despite sanity checks (fail on
                                        failure)
                            - 'off'   : do not attempt
                            - 'on'    : fail if sanity checks indicate no
                                        support

   Example:
    - ${0##*/} /dev/sda 1
      Resize partition 1 on /dev/sda

    - ${0##*/} --free-percent=10 /dev/sda 1
      Resize partition 1 on /dev/sda so that 10% of the disk is unallocated
";
    return;
}

sub bad_Usage {
    do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
        Usage();
    };
    error("\@ARGV");
exit 2;
    return;
}

sub lock_disk {
    my ($file) = @_;
    my $disk = "$_[0]";
    if (!("${DRY_RUN}" eq 0)) {
        return;
    }
    if (!(( -b "${disk}"))) {
        return;
    }
    $FLOCK_DISK_FD = q{9};
    debug(1, "FLOCK: try exec open fd 9, on failure exec exits this program");
open STDIN, '<', $disk or croak "Cannot read file: $OS_ERROR\n";
# Builtin command 'exec' not implemented
        $main_exit_code = system('rq', 'flock', 'flock', '-x', $FLOCK_DISK_FD) >> 8;
    if ($CHILD_ERROR != 0) {
                fail("Error while obtaining exclusive lock on $DISK");
    }
;
    debug(1, "FLOCK: $disk: obtained exclusive lock");
    return;
}

sub unlock_disk_and_settle {
    my ($file) = @_;
    my $disk = "$_[0]";
    my $settle = $_[1]-"1";
    if (!("${DRY_RUN}" eq 0)) {
        return;
    }
    if (!("${FLOCK_DISK_FD}" ne q{})) {
        return;
    }
    debug(1, "FLOCK: " . ${disk} . ": releasing exclusive lock");
    do {
local *STDERR;
open STDERR, '>', q{-} or croak "Cannot access file: $OS_ERROR\n";
# Builtin command 'exec' not implemented
    };
    if (do {
if ("${settle}" eq 1) {
        $main_exit_code = system('has_cmd', 'udevadm') >> 8;
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
        $CHILD_ERROR == 0
    }) {
                $main_exit_code = system('udevadm', 'settle') >> 8;
    }
    $FLOCK_DISK_FD = "";
    return;
}

sub sfdisk_restore_legacy {
    $main_exit_code = system('sfdisk', '--no-reread', ${DISK}, '-I', ${MBR_BACKUP}) >> 8;
    return;
}

sub sfdisk_restore {
    my $f = "";
    my $offset = "";
    my $fails = "0";
    for my $f (${MBR_BACKUP}, '*.bak') {
        if (!((-f "$f"))) {
            next;
        }
        $offset = ${f} =~ s/^.*-//sr;
        $offset = scalar reverse( (scalar reverse ${offset}) =~ s/^kab\.//r );
        if ("$offset" eq "$f") {
                            error("WARN: confused by file $f");
next;
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
                $main_exit_code = system('dd', "if=$f", "of=" . ${DISK}, 'seek', q{=}, eval { int($offset) } // "", 'bs', q{=}, q{1}, 'conv', q{=}, 'notrunc') >> 8;
        if ($CHILD_ERROR != 0) {
                            error("WARN: failed restore from $f");
                if (defined $fails) {
                    $fails = eval { int($fails+1) } // "";
                }
        }
;
    }
    $f = '*.bak';
return $fails;
    return;
}

sub sfdisk_worked_but_blkrrpart_failed {
    my ($file) = @_;
    my $ret = "$_[0]";
    my $output = "$_[1]";
if (!(my $grep_result_4;
my @grep_lines_4 = ();
my @grep_filtered_4 = grep { /Success.*\ wrote.*\ new.*\ partition/msxi } @grep_lines_4;
$grep_result_4 = join "\n", @grep_filtered_4;
    if (!($grep_result_4 =~ m{\n\z} || $grep_result_4 eq q{})) {
        $grep_result_4 .= "\n";
    }
$CHILD_ERROR = scalar @grep_filtered_4 > 0 ? 0 : 1;
$grep_result_4 = q{};)) {
my $grep_result_5;
my @grep_lines_5 = ();
my @grep_filtered_5 = grep { /BLKRRPART:\ Device\ or\ resource\ busy/msxi } @grep_lines_5;
$grep_result_5 = join "\n", @grep_filtered_5;
        if (!($grep_result_5 =~ m{\n\z} || $grep_result_5 eq q{})) {
            $grep_result_5 .= "\n";
        }
$CHILD_ERROR = scalar @grep_filtered_5 > 0 ? 0 : 1;
$grep_result_5 = q{};
return;
}
    else {
        if (!(my $grep_result_6;
my @grep_lines_6 = ();
my @grep_filtered_6 = grep { /The.*\ part.*\ table.*\ has.*\ been.*\ altered/msxi } @grep_lines_6;
$grep_result_6 = join "\n", @grep_filtered_6;
        if (!($grep_result_6 =~ m{\n\z} || $grep_result_6 eq q{})) {
            $grep_result_6 .= "\n";
        }
$CHILD_ERROR = scalar @grep_filtered_6 > 0 ? 0 : 1;
$grep_result_6 = q{};)) {
my $grep_result_7;
my @grep_lines_7 = ();
my @grep_filtered_7 = grep { /Re-reading.*\ partition.*\ table.*\ failed/msxi } @grep_lines_7;
$grep_result_7 = join "\n", @grep_filtered_7;
            if (!($grep_result_7 =~ m{\n\z} || $grep_result_7 eq q{})) {
                $grep_result_7 .= "\n";
            }
$CHILD_ERROR = scalar @grep_filtered_7 > 0 ? 0 : 1;
$grep_result_7 = q{};
return;
        }
    }
return $ret;
    return;
}

sub get_sfdisk_version {
    my ($ver) = @_;
    my $out;
    my $oifs = "$ENV{IFS}";
    my $ver = "";
    if ("$SFDISK_VERSION" ne q{}) {
        return q{0};
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    if (!("$SFDISK" ne q{})) {
                    $SFDISK_VERSION = q{0};
return q{0};
    }
        $out = do {
    my ($in_8, $out_8);
    my $pid_8 = open3($in_8, $out_8, '>&STDERR', 'sfdisk', '--version');
    close $in_8 or croak 'Close failed: $OS_ERROR';
    my $result_8 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_8> };
    close $out_8 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_8, 0;
    $result_8
};
    if ($CHILD_ERROR != 0) {
                    error("failed to get sfdisk version");
return q{1};
    }
;
# set -- not implemented
    my $IFS;
if ("$ver" =~ /^\[0-9\].*.\[0-9\].*.\[0-9\]$/msx or "$ver" =~ /^\[0-9\].\[0-9\].*$/msx) {
                $IFS = ".";
        # set -- not implemented
                $IFS = "$oifs";
                $SFDISK_VERSION = eval { int($_[0]*10000+$_[1]*100+(defined $ENV{3} && $ENV{3} ne q{} ? $ENV{3} : 0)) } // "";
        return q{0};    } elsif (1) {
                error("unexpected output in sfdisk --version [$out]");
        return q{1};    }
;
    return;
}

sub get_diskpart_path {
    my ($file) = @_;
    my $disk = "$_[0]";
    my $part = "$_[1]";
    my $dpart = "";
    $dpart = ${disk} . ${part};
if (( -b "$disk")) {
if (((-b "${disk}p${part}") && "${disk%[0-9]}" ne "${disk}")) {
            $dpart = ${disk} . "p" . ${part};
}
        else {
            if ("${disk#/dev/loop[0-9]}" ne "${disk}") {
                $dpart = ${disk} . "p" . ${part};
            }
        }
}
    else {
if ("$disk" =~ /^.*\[0-9\]$/msx) {
                        $dpart = ${disk} . "p" . ${part};
        }
    }
    my $_RET = "$dpart";
    return;
}

sub resize_sfdisk {
    my ($file) = @_;
    my $humanpt = ${TEMP_D} . "/recovery";
    my $mbr_backup = ${TEMP_D} . "/orig.save";
    my $restore_func = "";
    my $format = "$_[0]";
    my $change_out = $TEMP_D;
    my $/change.out;
    my $dump_out = $TEMP_D;
    my $/dump.out;
    my $new_out = $TEMP_D;
    my $/new.out;
    my $dump_mod = $TEMP_D;
    my $/dump.mod;
    my $tmp = ${TEMP_D} . "/tmp.out";
    my $err = ${TEMP_D} . "/err.out";
    my $mbr_max_512 = "4294967296";
    my $pt_start;
    my $pt_size;
    my $pt_end;
    my $max_end;
    my $new_size;
    my $change_info;
    my $dpart;
    my $sector_num;
    my $sector_size;
    my $disk_size;
    my $tot;
    my $out;
    my $excess_sectors;
    my $free_percent_sectors;
    my $remaining_free_sectors;
        my $LANG = q{C};
                do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', "$tmp"
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('rqe', 'sfd_list', 'sfdisk', '--list', '--unit=S', "$DISK") >> 8;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        if ($CHILD_ERROR != 0) {
                        fail("failed: sfdisk --list $DISK");
        }
;
    my $t;
    my $msg;
if ((${SFDISK_VERSION} < ${SFDISK_2_26})) {
                $out = do { my @_qx_cmd = (q(awk '$_[0] == "Units:" && $_[4] ~ /bytes/ { print $_[3] }' "$tmp")); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        if ($CHILD_ERROR != 0) {
                        fail("failed to read sfdisk output");
        }
;
if ("$out" eq q{}) {
            error("WARN: sector size not found in sfdisk output, assuming 512");
            $sector_size = '512';
}
        else {
            $sector_size = "$out";
        }
        my $_w;
        my $_cyl;
        my $_w1;
        my $_heads;
        my $_w2;
        my $sectors;
        my $_w3;
        my $t;
        my $s;
                $t = do {
    my ($in_9, $out_9);
    my $pid_9 = open3($in_9, $out_9, '>&STDERR', 'sfdisk', '--show-size', ${DISK});
    close $in_9 or croak 'Close failed: $OS_ERROR';
    my $result_9 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_9> };
    close $out_9 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_9, 0;
    $result_9
};
        if ($CHILD_ERROR != 0) {
                        fail("failed: sfdisk --show-size $DISK");
        }
;
        $disk_size = eval { int($t*1024) } // "";
        $sector_num = eval { int($disk_size/$sector_size) } // "";
        $msg = "disk size '$disk_size' not evenly div by sector size '$sector_size'";
        if (!(((eval { int($disk_size%$sector_size) } // "") == 0))) {
                        error("WARN: $msg");
        }
        $restore_func = 'sfdisk_restore_legacy';
}
    else {
        my $_x;
open STDIN, '<', "$tmp" or croak "Cannot read file: $OS_ERROR\n";
$_x = <>;
chomp $_x;
$CHILD_ERROR = defined($_x) ? 0 : 1;
        $sector_size = eval { int($disk_size/$sector_num) } // "";
        $restore_func = 'sfdisk_restore';
    }
;
    debug(1, "$sector_num sectors of $sector_size. total size=" . ${disk_size} . " bytes");
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', ${dump_out}
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('rqe', 'sfd_dump', 'sfdisk', '--unit=S', '--dump', ${DISK}) >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
                fail("failed to dump sfdisk info for " . ${DISK});
    }
;
    $RESTORE_HUMAN = "$dump_out";
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', "$humanpt"
      or die "Cannot access file: $OS_ERROR\n";
            say "## sfdisk --unit=S --dump " . ${DISK};
print do { my $cat_chunk = q{}; if ( open my $fh, '<', ${dump_out} ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . ${dump_out} . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if (!(($? == 0))) {
                fail("failed to save sfdisk -d output");
    }
    $RESTORE_HUMAN = "$humanpt";
    debugcat(1, "$humanpt");
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', ${dump_mod}
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
my @sed_lines_12 = split /\n/, $;
my @sed_result_12;
foreach my $line (@sed_lines_12) {
chomp $line;
push @sed_result_12, $line;
}
$ = join "\n", @sed_result_12;

        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
                fail("sed failed on dump output");
    }
;
    get_diskpart_path($DISK, $PART);
    $dpart = "$ENV{_RET}";
        if (do {
if (do {
if (do {
$pt_start = do { my @_qx_cmd = ("awk '$_[0] == pt { print $_[3] }' \"pt=${dpart}\" < \"${dump_mod}\""); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $CHILD_ERROR == 0
}) {
        $pt_size = do { my @_qx_cmd = ("awk '$_[0] == pt { print $_[5] }' \"pt=${dpart}\" < \"${dump_mod}\""); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
}
    $CHILD_ERROR == 0
}) {
    ("${pt_start}" ne q{} && "${pt_size}" ne q{})
}
        $CHILD_ERROR == 0
    }) {
                $pt_end = eval { int($pt_size + $pt_start - 1) } // "";
    }
    if ($CHILD_ERROR != 0) {
                fail("failed to get start and end for " . ${dpart} . " in " . ${DISK});
    }
        if (do {
$max_end = do { my @_qx_cmd = (q[awk "\$_[2] == \"start\" { if(\$_[3] >= pt_end && \$_[3] < min)
		{ min = \$_[3] } } END { printf(\"%s\\n\",min); }" min = $sector_num pt_end = $pt_end "${dump_mod}"]); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $CHILD_ERROR == 0
    }) {
        "${max_end}" ne q{}
    }
    if ($CHILD_ERROR != 0) {
                fail("failed to get max_end for partition " . ${PART});
    }
    $max_end = eval { int($max_end - 1) } // "";
if ("$format" eq "gpt") {
        my @sed_lines_13 = split /\n/, $;
my @sed_result_13;
foreach my $line (@sed_lines_13) {
chomp $line;
push @sed_result_13, $line;
}
$ = join "\n", @sed_result_13;

        if ($CHILD_ERROR != 0) {
                        fail("failed to remove last-lba from output");
        }
;
    }
    my $mbr_max_sectors;
if ("$format" eq "dos") {
        $mbr_max_sectors = eval { int($mbr_max_512*do { chomp(my $_r = qx'(sector_size/512)'); $_r; }) } // "";
if (($max_end > $mbr_max_sectors)) {
            $max_end = $mbr_max_sectors;
        }
        if (((eval { int($disk_size/512) } // "") > $mbr_max_512)) {
                        debug(0, "WARNING: MBR/dos partitioned disk is larger than 2TB.", "Additional space will go unused.");
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
    }
;
    my $gpt_second_size = "33";
if ((${max_end} > (eval { int($sector_num-$gpt_second_size) } // ""))) {
        debug(1, "padding " . ${gpt_second_size} . " sectors for gpt secondary header");
        $max_end = eval { int($sector_num - $gpt_second_size - 1) } // "";
    }
if ("${free_percent}" ne q{}) {
        my $free_percent;
        $free_percent_sectors = eval { int($sector_num/100*$free_percent) } // "";
if ("$format" eq "dos") {
if (((eval { int($disk_size/512) } // "") >= (eval { int($mbr_max_512+$free_percent_sectors) } // ""))) {
                debug(1, "WARNING: Additional unused space on MBR/dos partitioned disk", "is larger than requested percent of overprovisioning.");
}
            else {
                if (($sector_num > $mbr_max_512)) {
                    $excess_sectors = eval { int($sector_num-$mbr_max_512) } // "";
                    $remaining_free_sectors = eval { int($free_percent_sectors - $excess_sectors) } // "";
                    debug(1, "reserving " . ${remaining_free_sectors} . " sectors from MBR maximum for overprovisioning");
                    $max_end = eval { int($max_end - $remaining_free_sectors) } // "";
}
                else {
                    debug(1, "reserving " . ${free_percent_sectors} . " sectors (" . ${free_percent} . "%) for overprovisioning");
                    $max_end = eval { int($max_end-$free_percent_sectors) } // "";
                }
            }
if ((${max_end} < ${pt_end})) {
                nochange("partition " . ${PART} . " could not be grown while leaving", ${free_percent} . "% (" . ${free_percent_sectors} . " sectors) free on device");
return;
            }
}
        else {
            debug(1, "reserving " . ${free_percent_sectors} . " sectors (" . ${free_percent} . "%) for overprovisioning");
            $max_end = eval { int($max_end-$free_percent_sectors) } // "";
if ((${max_end} < ${pt_end})) {
                nochange("partition " . ${PART} . " could not be grown while leaving", ${free_percent} . "% (" . ${free_percent_sectors} . " sectors) free on device");
return;
            }
        }
    }
    debug(1, "max_end=" . ${max_end} . " tot=" . ${sector_num} . " pt_end=" . ${pt_end}, "pt_start=" . ${pt_start} . " pt_size=" . ${pt_size});
    if (((eval { int($pt_end) } // "") == ${max_end})) {
                    nochange("partition " . ${PART} . " is size " . ${pt_size} . ". it cannot be grown");
return;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    if (((eval { int($pt_end+($FUDGE/$sector_size) } // "")))) > ${max_end})) {
                    nochange("partition " . ${PART} . " could only be grown by", (do {
    my ($in_14, $out_14);
    my $pid_14 = open3($in_14, $out_14, '>&STDERR', '${max_end}-${pt_end}');
    close $in_14 or croak 'Close failed: $OS_ERROR';
    my $result_14 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_14> };
    close $out_14 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_14, 0;
    $result_14
}) . " [fudge=" . (do {
    my ($in_15, $out_15);
    my $pid_15 = open3($in_15, $out_15, '>&STDERR', '${FUDGE}/$sector_size');
    close $in_15 or croak 'Close failed: $OS_ERROR';
    my $result_15 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_15> };
    close $out_15 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_15, 0;
    $result_15
}) . "]");
return;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    $new_size = eval { int($max_end - $pt_start + 1) } // "";
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', ${new_out}
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
my @sed_lines_16 = split /\n/, $;
my @sed_result_16;
foreach my $line (@sed_lines_16) {
chomp $line;
push @sed_result_16, $line;
}
$ = join "\n", @sed_result_16;

        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
                fail("failed to change size in output");
    }
;
    $change_info = "partition=" . ${PART} . " start=" . ${pt_start};
    $change_info = ${change_info} . " old: size=" . ${pt_size} . " end=" . ${pt_end};
    $change_info = ${change_info} . " new: size=" . ${new_size} . " end=" . ${max_end};
if ((${DRY_RUN} != 0)) {
        say "CHANGE: " . ${change_info};
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
                say "# === old sfdisk -d ===";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', ${dump_out} ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . ${dump_out} . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
                say "# === new sfdisk -d ===";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', ${new_out} ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . ${new_out} . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
        };
exit 0;
    }
    $MBR_BACKUP = ${mbr_backup};
        $LANG = q{C};
open STDIN, '<', ${new_out} or croak "Cannot read file: $OS_ERROR\n";
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', ${change_out}
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('sfdisk', '--no-reread', ${DISK}, '--force', '-O', ${mbr_backup}) >> 8;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    my $ret = $?;
    if (!(($ret == 0))) {
                $RESTORE_FUNC = ${restore_func};
    }
if (($ret == 0)) {
        debug(1, "resize of " . ${DISK} . " returned 0.");
if (($VERBOSITY > 2)) {
            do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
my @sed_lines_19 = split /\n/, $;
my @sed_result_19;
foreach my $line (@sed_lines_19) {
chomp $line;
push @sed_result_19, $line;
}
$ = join "\n", @sed_result_19;

            };
        }
}
    else {
        if ((!(        $CHILD_ERROR = 0) && !(        sfdisk_worked_but_blkrrpart_failed("$ret", ${change_out})))) {
            debug(1, "sfdisk failed, but likely only because of blkrrpart");
}
        else {
            error("attempt to resize " . ${DISK} . " failed. sfdisk output below:");
            do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
my @sed_lines_20 = split /\n/, $;
my @sed_result_20;
foreach my $line (@sed_lines_20) {
chomp $line;
push @sed_result_20, $line;
}
$ = join "\n", @sed_result_20;

            };
            fail("failed to resize");
        }
    }
        $main_exit_code = system('rq', 'pt_update', 'pt_update', "$DISK", "$PART") >> 8;
    if ($CHILD_ERROR != 0) {
                fail("pt_resize failed");
    }
;
    $RESTORE_FUNC = "";
    changed(${change_info});
return;
    return;
}

sub gpt_restore {
    $main_exit_code = system('sgdisk', '-l', ${GPT_BACKUP}, ${DISK}) >> 8;
    return;
}

sub resize_sgdisk {
    my ($file) = @_;
    $GPT_BACKUP = ${TEMP_D} . "/pt.backup";
    my $pt_info = ${TEMP_D} . "/pt.info";
    my $pt_pretend = ${TEMP_D} . "/pt.pretend";
    my $pt_data = ${TEMP_D} . "/pt.data";
    my $out = ${TEMP_D} . "/out";
    my $dev = "disk=" . ${DISK} . " partition=" . ${PART};
    my $pt_start;
    my $pt_end;
    my $pt_size;
    my $last;
    my $pt_max;
    my $code;
    my $guid;
    my $name;
    my $new_size;
    my $old;
    my $new;
    my $change_info;
    my $sector_size;
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', ${pt_info}
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('rqe', 'sgd_info', 'sgdisk', "--info=" . ${PART}, '--print', ${DISK}) >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to dump original sgdisk info");
    }
;
    $RESTORE_HUMAN = ${pt_info};
        if (do {
$sector_size = do { my @_qx_cmd = (q[awk "
		\$0 ~ /^Logical sector size:.*bytes/ { print \$_[3]; exit(0); }
		\$0 ~ /^Sector size \\(logical\\):/ { print \$_[3]; exit(0); }
		\$0 ~ /^Sector size \\(logical\\/physical\\):/ {
		    sub(/\\/.*/, \"\", \$_[3]); print \$_[3]; exit(0); }" "$pt_info"]); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $CHILD_ERROR == 0
    }) {
        "$sector_size" ne q{}
    }
    if ($CHILD_ERROR != 0) {
                    $sector_size = '512';
            error("WARN: did not find sector size, assuming 512");
    }
    debug(1, "$dev: original sgdisk info:");
    debugcat(1, ${pt_info});
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', ${pt_pretend}
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('rqe', 'sgd_pretend', 'sgdisk', '--pretend', '--move-second-header', '--print', ${DISK}) >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to dump pretend sgdisk info");
    }
;
    debug(1, "$dev: pretend sgdisk info");
    debugcat(1, ${pt_pretend});
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', ${pt_data}
      or die "Cannot access file: $OS_ERROR\n";
        my $tmp = do {
my @lines = split /\n/, $;
my @result;
foreach my $line (@lines) {
    chomp $line;
    if ($line =~ /^\s*$/msx) { next; }
    my @fields = split /\s+/msx, $line;
    if (!(found)) { next; }
    push @result, (} ; $fields[0] == "Number" { found = 1 . "\n");
}
$ = join "", @result;

        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to parse pretend sgdisk info");
    }
;
        if (do {
$pt_start = do { my @_qx_cmd = (q(awk '$_[0] == ' "${PART}" ' { print $_[1] }' "${pt_data}")); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $CHILD_ERROR == 0
    }) {
        "${pt_start}" ne q{}
    }
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to get start sector");
    }
        if (do {
$pt_end = do { my @_qx_cmd = (q(awk '$_[0] == ' "${PART}" ' { print $_[2] }' "${pt_data}")); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $CHILD_ERROR == 0
    }) {
        "${pt_end}" ne q{}
    }
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to get end sector");
    }
    $pt_size = (do {
    my ($in_22, $out_22);
    my $pid_22 = open3($in_22, $out_22, '>&STDERR', '${pt_end} - ${pt_start} + 1');
    close $in_22 or croak 'Close failed: $OS_ERROR';
    my $result_22 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_22> };
    close $out_22 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_22, 0;
    $result_22
});
        if (do {
$last = do { my @_qx_cmd = (q(awk '/last usable sector is/ { print $NF }' "${pt_pretend}")); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $CHILD_ERROR == 0
    }) {
        "${last}" ne q{}
    }
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to get last usable sector");
    }
        if (do {
$pt_max = do { my @_qx_cmd = (q[awk "{ if (\$_[1] >= pt_end && \$_[1] < min) { min = \$_[1] } } END \\
		{ print min-1 }" min = "${last}" pt_end = "${pt_end}" "${pt_data}"]); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $CHILD_ERROR == 0
    }) {
        "${pt_max}" ne q{}
    }
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to find max end sector");
    }
    debug(1, ${dev} . ": pt_start=" . ${pt_start} . " pt_end=" . ${pt_end}, "pt_size=" . ${pt_size} . " pt_max=" . ${pt_max} . " last=" . ${last});
    if ((${pt_end} == ${pt_max})) {
                    nochange(${dev} . ": size=" . ${pt_size} . ", it cannot be grown");
return;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    if (((eval { int($pt_end + $FUDGE/$sector_size) } // "") > ${pt_max})) {
                    nochange(${dev} . ": could only be grown by", (do {
    my ($in_23, $out_23);
    my $pid_23 = open3($in_23, $out_23, '>&STDERR', '${pt_max} - ${pt_end}');
    close $in_23 or croak 'Close failed: $OS_ERROR';
    my $result_23 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_23> };
    close $out_23 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_23, 0;
    $result_23
}) . " [fudge=" . (do {
    my ($in_24, $out_24);
    my $pid_24 = open3($in_24, $out_24, '>&STDERR', '${FUDGE}/$sector_size');
    close $in_24 or croak 'Close failed: $OS_ERROR';
    my $result_24 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_24> };
    close $out_24 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_24, 0;
    $result_24
}) . "]");
return;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    $code = do { my @_qx_cmd = (q(awk '/^Partition GUID code:/ { print $_[3] }' "${pt_info}")); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $guid = do { my @_qx_cmd = (q(awk '/^Partition unique GUID:/ { print $_[3] }' "${pt_info}")); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $name = do { my @_qx_cmd = (q[awk '/^Partition name:/ { gsub(/' ' "/, \"\") ; \\
		if (NF >= 3) print substr(\$0, index(\$0, \$_[2])) }" "${pt_info}"]); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    if (!(("${code}" ne q{} && "${guid}" ne q{}))) {
                fail(${dev} . ": failed to parse sgdisk details");
    }
    debug(1, ${dev} . ": code=" . ${code} . " guid=" . ${guid} . " name='" . ${name} . "'");
    my $wouldrun = "";
    if (($DRY_RUN != 0)) {
                $wouldrun = "would-run";
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    $new_size = eval { int($pt_max - $pt_start + 1) } // "";
    $change_info = "partition=" . ${PART} . " start=" . ${pt_start};
    $change_info = ${change_info} . " old: size=" . ${pt_size} . " end=" . ${pt_end};
    $change_info = ${change_info} . " new: size=" . ${new_size} . " end=" . ${pt_max};
        $main_exit_code = system('rq', 'sgd_backup', $wouldrun, 'sgdisk', "--backup=" . ${GPT_BACKUP}, ${DISK}) >> 8;
    if ($CHILD_ERROR != 0) {
                fail(${dev} . ": failed to backup the partition table");
    }
;
        if (do {
$main_exit_code = system('rq', 'sgdisk_mod', $wouldrun, 'sgdisk', '--move-second-header', "--delete=" . ${PART}, "--new=" . ${PART} . ":" . ${pt_start} . ":" . (do {
    my ($in_25, $out_25);
    my $pid_25 = open3($in_25, $out_25, '>&STDERR', 'pt_max-1');
    close $in_25 or croak 'Close failed: $OS_ERROR';
    my $result_25 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_25> };
    close $out_25 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_25, 0;
    $result_25
}), "--typecode=" . ${PART} . ":" . ${code}, "--partition-guid=" . ${PART} . ":" . ${guid}, "--change-name=" . ${PART} . ":" . ${name}, ${DISK}) >> 8;
        $CHILD_ERROR == 0
    }) {
                $main_exit_code = system('rq', 'pt_update', $wouldrun, 'pt_update', "$DISK", "$PART") >> 8;
    }
    if ($CHILD_ERROR != 0) {
                    $RESTORE_FUNC = 'gpt_restore';
            fail(${dev} . ": failed to repartition");
    }
    if ((${DRY_RUN} != 0)) {
                    change(${change_info});
return;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    changed(${change_info});
return;
    return;
}

sub kver_to_num {
    my ($file) = @_;
    my $kver = "$_[0]";
    my $maj;
    my $min;
    my $mic;
    $kver = q{};
    $main_exit_code = system('bash', '.0.0') >> 8;
    $maj = q{};
    $kver = ${kver} =~ s/^.*?\.//r;
    $min = q{};
    $kver = ${kver} =~ s/^.*?\.//r;
    $mic = q{};
    my $_RET = eval { int($maj*1000*1000+$min*1000+$mic) } // "";
    return;
}

sub kver_cmp {
    my ($file) = @_;
    my $op = "$_[1]";
    my $n1 = "";
    my $n2 = "";
    kver_to_num("$_[0]");
    $n1 = "$ENV{_RET}";
    kver_to_num("$_[2]");
    $n2 = "$ENV{_RET}";
    $main_exit_code = system('test', $n1, $op, $n2) >> 8;
    return;
}

sub rq {
    my ($file) = @_;
    my $label = "$_[0]";
    my $ret = "";
    my $efile = "";
    $efile = "$TEMP_D/$label.err";
# Builtin command 'shift' not implemented
    my $rlabel = "running";
    if (do {
if ("$_[0]" eq "would-run") {
        $rlabel = "would-run";
    $CHILD_ERROR = 0;
} else {
    $CHILD_ERROR = 1;
}
        $CHILD_ERROR == 0
    }) {
        # Builtin command 'shift' not implemented
    }
    my $cmd = "";
    my $x = "";
    for my $x (@ARGV) {
        if (("${x#* }" ne "$x" || "${x#* \"}" ne "$x")) {
                        $x = "'$x'";
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
        $cmd = "$cmd $x";
    }
    $cmd = ${cmd} =~ s/^ //r;
    debug(2, ${rlabel} . "[$label][$_capture]", "$cmd");
    if ("$rlabel" eq "would-run") {
        return q{0};
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
if ("${_capture}" eq "erronly") {
        do {
local *STDERR;
open STDERR, '>', "$TEMP_D/$label.err" or croak "Cannot access file: $OS_ERROR\n";
            $CHILD_ERROR = 0;
        };
        $ret = $?;
}
    else {
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', "$TEMP_D/$label.err"
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
            my $tmp = do {
            $CHILD_ERROR = 0;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        $ret = $?;
    }
if (($ret != 0)) {
        error("failed [$label:$ret]", "\@ARGV");
        do {
local *STDERR;
open STDERR, '>&', STDERR or die "Cannot dup stderr: $OS_ERROR\n";
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "$efile" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$efile" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
        };
    }
return $ret;
    return;
}

sub rqe {
    my $_capture = "erronly";
    rq("\@ARGV");
    return;
}

sub verify_ptupdate {
    my ($file) = @_;
    my $input = "$_[0]";
    my $found = "";
    my $reason = "";
    my $kver = "";
    my $_RET;
if ("$input" eq "off") {
        $_RET = "false";
return q{0};
    }
;
    my $ret;
    my $out;
if (!(    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', 'partx') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };)) {
        my $out = "";
        my $ret = "0";
        $out = do { my @_qx_cmd = ("partx --help 2>&1"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
        $ret = $?;
if (($ret == 0)) {
            do {
                my $output_27 = q{};
                my $output_printed_27;
                my $pipeline_success_27 = 1;
                $output_27 .= $out . "\n";
if ( !($output_27 =~ m{\n\z}) ) { $output_27 .= "\n"; }

                                carp "grep: no pattern specified";
                exit 1;
                $output_27 = q{};
                if ((scalar @grep_filtered_27_1) == 0) {
                    $pipeline_success_27 = 0;
                }
                if ($output_27 ne q{} && !defined $output_printed_27) {
                    print $output_27;
                    if (!($output_27 =~ m{\n\z})) {
                        print "\n";
                    }
                }
                if ( !$pipeline_success_27 ) { $main_exit_code = 1; }
                }
            if ($CHILD_ERROR != 0) {
                                    $reason = "partx has no '--update' flag in usage.";
                    $found = "off";
            }
;
}
        else {
            $reason = "'partx --help' returned $ret. assuming it is old.";
            $found = "off";
        }
}
    else {
        $reason = "no 'partx' command";
        $found = "off";
    }
;
if ("$found" eq q{}) {
if ("$(uname)" ne "Linux") {
            $reason = "Kernel is not Linux per uname.";
            $found = "off";
        }
    }
if ("$found" eq q{}) {
                $kver = do { use POSIX qw(uname); my ($__sys, $__node, $__rel, $__ver, $__mach) = POSIX::uname(); my @__parts; push @__parts, $__rel; join(" ", @__parts) . "\n"; };
        if ($CHILD_ERROR != 0) {
                        debug(1, "uname -r failed!");
        }
;
if (        kver_cmp((defined (defined ${kver} && ${kver} ne q{} ? ${kver} : '0.0.0') && (defined ${kver} && ${kver} ne q{} ? ${kver} : '0.0.0') ne q{} ? (defined ${kver} && ${kver} ne q{} ? ${kver} : '0.0.0') : '0.0.0'), '-ge', '3.8.0')) {
            $reason = "Kernel '$kver' < 3.8.0.";
            $found = "off";
        }
    }
if ("$found" eq q{}) {
        $_RET = "true";
return q{0};
    }
if ("$input" eq 'on') {
                error("$reason");
        return q{1};    } elsif ("$input" eq 'auto') {
                $_RET = "false";
                debug(1, "partition update disabled: $reason");
        return q{0};    } elsif ("$input" eq 'force') {
                $_RET = "true";
                error("WARNING: ptupdate forced on even though: $reason");
        return q{0};    }
    error("unknown input '$input'");
return q{1};
    return;
}

sub pt_update {
    my ($file) = @_;
    my $dev = "$_[0]";
    my $part = "$_[1]";
    my $update = (defined (defined $_[2] && $_[2] ne q{} ? $_[2] : '$PT_UPDATE') && (defined $_[2] && $_[2] ne q{} ? $_[2] : '$PT_UPDATE') ne q{} ? (defined $_[2] && $_[2] ne q{} ? $_[2] : '$PT_UPDATE') : '$PT_UPDATE');
if (    $CHILD_ERROR = 0) {
return q{0};
    }
    if (!(( -b "$dev"))) {
        return q{0};
    }
    $main_exit_code = system('partx', '--update', '--nr', "$part", "$dev") >> 8;
    return;
}

sub has_cmd {
    do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', $_[0]) >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    return;
}

sub resize_sgdisk_gpt {
    resize_sgdisk('gpt');
    return;
}

sub resize_sgdisk_dos {
    fail("unable to resize dos label with sgdisk");
    return;
}

sub resize_sfdisk_gpt {
    resize_sfdisk('gpt');
    return;
}

sub resize_sfdisk_dos {
    resize_sfdisk('dos');
    return;
}

sub get_table_format {
    my ($file) = @_;
    my $out = "";
    my $disk = "$_[0]";
    my $_RET;
if ((((!(    has_cmd('blkid')) && !(do {
        my $output_28 = q{};
        my $output_printed_28;
        my $pipeline_success_28 = 1;
                my ($in_29, $out_29);
        my $pid_29 = open3($in_29, $out_29, '>&STDERR', 'blkid', '--version');
        close $in_29 or croak 'Close failed: $OS_ERROR';
        $output_28 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_29> };
        close $out_29 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_29, 0;

                my $grep_result_28_1;
        my @grep_lines_28_1 = split /\n/msx, $output_28;
        my @grep_filtered_28_1 = grep { /util-linux/msx } @grep_lines_28_1;
        $grep_result_28_1 = join "\n", @grep_filtered_28_1;
        if (!($grep_result_28_1 =~ m{\n\z} || $grep_result_28_1 eq q{})) {
        $grep_result_28_1 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_28_1 > 0 ? 0 : 1;
        $grep_result_28_1 = q{};
        $output_28 = q{};
        if ((scalar @grep_filtered_28_1) == 0) {
            $pipeline_success_28 = 0;
        }
        if ($output_28 ne q{} && !defined $output_printed_28) {
            print $output_28;
            if (!($output_28 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_28 ) { $main_exit_code = 1; }
        })) && !(    $out = do {
    my ($in_30, $out_30);
    my $pid_30 = open3($in_30, $out_30, '>&STDERR', 'blkid', '-o', 'value', '-s', 'PTTYPE', "$disk");
    close $in_30 or croak 'Close failed: $OS_ERROR';
    my $result_30 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_30> };
    close $out_30 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_30, 0;
    $result_30
})) && ("$out" eq "dos" || "$out" eq "gpt"))) {
        $_RET = "$out";
return;
    }
;
    $_RET = "dos";
if ("$SFDISK" eq q{}) {
                $out = do {
    my ($in_31, $out_31);
    my $pid_31 = open3($in_31, $out_31, '>&STDERR', 'sgdisk', '--print', "$disk");
    close $in_31 or croak 'Close failed: $OS_ERROR';
    my $result_31 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_31> };
    close $out_31 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_31, 0;
    $result_31
};
        if ($CHILD_ERROR != 0) {
                            error("Could not determine partition table format of $disk", "with 'sgdisk --print $disk'");
return q{1};
        }
;
if ("$out" =~ /^.*\ valid\ MBR\ .*$/msx) {
                        $_RET = "dos";
        } elsif (1) {
                        $_RET = "gpt";
        }
return;
}
    else {
        if (((${SFDISK_VERSION} < ${SFDISK_2_26}) && !(        $out = do { my @_qx_cmd = ("sfdisk --id --force \"$disk\" 1 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; }))) {
if ("$out" eq "ee") {
                $_RET = "gpt";
}
            else {
                $_RET = "dos";
            }
return;
}
        else {
            if (!(            $out = do {
    my ($in_32, $out_32);
    my $pid_32 = open3($in_32, $out_32, '>&STDERR', 'sfdisk', '--list', "$disk");
    close $in_32 or croak 'Close failed: $OS_ERROR';
    my $result_32 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_32> };
    close $out_32 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_32, 0;
    $result_32
};)) {
                $out = do { my $result_33 = qx{bash -c q{echo "$out" | sed -e '/Disklabel type/!d' -e 's/.*: //'} }; chomp $result_33; $result_33; };
if ("$out" eq 'gpt' or "$out" eq 'dos') {
                                        $_RET = "$out";
                } elsif (1) {
                                        error("WARN: unknown label $out");
                }
            }
        }
    }
    return;
}

sub get_resizer {
    my ($file) = @_;
    my $format = "$_[0]";
    my $user = (defined $_[1] && $_[1] ne q{} ? $_[1] : '"auto"');
    my $_RET;
if ("$user" eq 'sgdisk') {
                $_RET = "resize_sgdisk_$format";
        return;    } elsif ("$user" eq 'sfdisk') {
                $_RET = "resize_sfdisk_$format";
        return;    } elsif ("$user" eq 'auto') {
                $main_exit_code = system('bash', ':') >> 8;
    } elsif (1) {
                error("unexpected value '$user' for growpart resizer");
        return q{1};    }
;
if ("$format" eq "dos") {
        if (!("$SFDISK" ne q{})) {
                            error("sfdisk is required for resizing dos/MBR partition table.");
return q{1};
        }
        $_RET = "resize_sfdisk_dos";
return q{0};
    }
if ((${SFDISK_VERSION} >= ${SFDISK_V_WORKING_GPT})) {
        $_RET = "resize_sfdisk_gpt";
}
    else {
        if (!(        has_cmd('sgdisk'))) {
            $_RET = "resize_sgdisk_$format";
}
        else {
            error("no tools available to resize disk with '$format'");
return q{1};
        }
    }
return q{0};
    return;
}

sub maybe_lvm_resize {
    my ($file) = @_;
    my $disk = "$_[0]";
    my $part = "$_[1]";
    my $partpath = "";
    my $ret = "";
    my $out = "";
    my $wouldrun = "";
    if (($DRY_RUN != 0)) {
                $wouldrun = "would-run";
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
        has_cmd('lvm');
    if ($CHILD_ERROR != 0) {
                    debug(2, "No lvm command, cannot attempt lvm resize of disk '$disk' part '$part'");
return q{0};
    }
;
        get_diskpart_path("$_[0]", "$_[1]");
    if ($CHILD_ERROR != 0) {
                    error("could not determine partition path for disk '$DISK' part '$part'");
return q{1};
    }
;
    $partpath = "$ENV{_RET}";
# set -- not implemented
# set lvm not implemented
# set pvs not implemented
# set --nolocking not implemented
# set --readonly not implemented
# set -o pvname not implemented
# set pvname not implemented
    debug(2, "executing: \@ARGV");
    $out = do { my @_qx_cmd = ("\"$@\" 2>&1"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    $ret = $?;
if ("$ret" eq '5') {
                debug(1, "$partpath is not an lvm pv");
        return q{0};    } elsif ("$ret" eq '0') {
                $main_exit_code = system('bash', ':') >> 8;
    } elsif (1) {
                error("failed to execute [$ret] '\@ARGV'");
                error("$out");
        return q{1};    }
        rq('lvm_resize', $wouldrun, 'lvm', 'pvresize', "$partpath");
    if ($CHILD_ERROR != 0) {
                    error("Failed to resize lvm pv $partpath");
return q{1};
    }
;
return q{0};
    return;
}
my $pt_update = "auto";
my $resizer = (defined ($ENV{GROWPART_RESIZER} // q{}) && ($ENV{GROWPART_RESIZER} // q{}) ne q{} ? ($ENV{GROWPART_RESIZER} // q{}) : '"auto"');
my $free_percent;
while ( scalar(@ARGV) != 0 ) {
    $cur = $1;
    $next = $2;
if ("$cur" eq '-h' or "$cur" eq '--help') {
                Usage();
        exit 0;
    } elsif ("$cur" eq '--free-percent' or "$cur" =~ /^--free-percent=.*$/msx) {
        if ("${cur#--free-percent=}" ne "$cur") {
            $next = (${cur} =~ s/^--free-percent=//r =~ s/^--free-percent=//r);
}
        else {
# Builtin command 'shift' not implemented
        }
        if ((!(        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
($next > 0)        }) && !(        do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
($next < 100)        }))) {
            $free_percent = $next;
}
        else {
            fail("unknown/invalid --free-percent option: $next");
        }
    } elsif ("$cur" eq '--fudge') {
                $FUDGE = $next;
        # Builtin command 'shift' not implemented
    } elsif ("$cur" eq '-N' or "$cur" eq '--dry-run') {
                $DRY_RUN = q{1};
    } elsif ("$cur" eq '-u' or "$cur" eq '--update' or "$cur" =~ /^--update=.*$/msx) {
        if ("${cur#--update=}" ne "$cur") {
            $next = (${cur} =~ s/^--update=//r =~ s/^--update=//r);
}
        else {
# Builtin command 'shift' not implemented
        }
        if ("$next" eq 'off' or "$next" eq 'auto' or "$next" eq 'force' or "$next" eq 'on') {
                        $pt_update = $next;
        } elsif (1) {
                        fail("unknown --update option: $next");
        }
    } elsif ("$cur" eq '-v' or "$cur" eq '--verbose') {
                $VERBOSITY = eval { int($VERBOSITY+1) } // "";
    } elsif ("$cur" eq '--') {
        # Builtin command 'shift' not implemented
        last;    } elsif ("$cur" =~ /^-.*$/msx) {
                fail("unknown option " . ${cur});
    } elsif (1) {
        if ("${DISK}" eq q{}) {
            $DISK = $cur;
}
        else {
            if (!("${PART}" eq q{})) {
                                fail("confused by arg " . ${cur});
            }
            $PART = $cur;
        }
    }
# Builtin command 'shift' not implemented
}
if (!("${DISK}" ne q{})) {
        bad_Usage("must supply disk and partition-number");
}
if (!("${PART}" ne q{})) {
        bad_Usage("must supply partition-number");
}
if (!((-e "${DISK}"))) {
        fail(${DISK} . ": does not exist");
}
if (do {
has_cmd('sfdisk');
    $CHILD_ERROR == 0
}) {
        $SFDISK = 'sfdisk';
}
if ($CHILD_ERROR != 0) {
        $SFDISK = "";
}
if (do {
has_cmd('sgdisk');
    $CHILD_ERROR == 0
}) {
        $SGDISK = 'sgdisk';
}
if ($CHILD_ERROR != 0) {
        $SGDISK = "";
}
if (!(("$SGDISK" ne q{} || "$SFDISK" ne q{}))) {
        fail("Did not have sfdisk or sgdisk in PATH.");
}
get_sfdisk_version();
if ($CHILD_ERROR != 0) {
        fail();
}
my $real_disk;
if ((-l "${DISK}")) {
        has_cmd('readlink');
    if ($CHILD_ERROR != 0) {
                fail(${DISK} . " is a symlink, but 'readlink' command not available.");
    }
;
        $real_disk = do {
    my ($in_34, $out_34);
    my $pid_34 = open3($in_34, $out_34, '>&STDERR', 'readlink', '-f', ${DISK});
    close $in_34 or croak 'Close failed: $OS_ERROR';
    my $result_34 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_34> };
    close $out_34 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_34, 0;
    $result_34
};
    if ($CHILD_ERROR != 0) {
                fail("unable to resolve " . ${DISK});
    }
;
    debug(1, ${DISK} . " resolved to " . ${real_disk});
    $DISK = $real_disk;
}
if (!("${PART#*[!0-9]}" eq "${PART}")) {
        fail("partition-number must be a number");
}
verify_ptupdate("$pt_update");
if ($CHILD_ERROR != 0) {
        fail();
}
$PT_UPDATE = $_RET;
debug(1, "update-partition set to $PT_UPDATE");
if (do {
mktemp_d();
    $CHILD_ERROR == 0
}) {
        $TEMP_D = ($ENV{_RET} // q{});
}
if ($CHILD_ERROR != 0) {
        fail("failed to make temp dir");
}
END { local $INPUT_RECORD_SEPARATOR = undef; my $end_out = qx'cleanup 2>&1'; print $end_out if $end_out ne q{}; }
get_table_format("$DISK");
if ($CHILD_ERROR != 0) {
        fail();
}
my $format = $_RET;
get_resizer("$format", "$resizer");
if ($CHILD_ERROR != 0) {
        fail("failed to get a resizer for format '$format'");
}
$resizer = $_RET;
lock_disk($DISK);
debug(1, "resizing $PART on $DISK using $resizer");
my $ret = $?;
unlock_disk_and_settle($DISK);
if (("$RESIZE_RESULT" eq "CHANGED" || "$RESIZE_RESULT" eq "CHANGE")) {
        maybe_lvm_resize("$DISK", "$PART");
    if ($CHILD_ERROR != 0) {
                fail("lvm resize failed.");
    }
;
}


exit $main_exit_code;

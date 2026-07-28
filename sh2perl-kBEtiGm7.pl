#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

# shopt -s dotglob not implemented
$main_exit_code = system('bash', 'nullglob') >> 8;
my $TOOL = 'blkdeactivate';
my $DEV_DIR = "/dev";
my $SYS_BLK_DIR = "/sys/block";
my $MDADM = "/sbin/mdadm";
my $MOUNTPOINT = "/bin/mountpoint";
my $MPATHD = "/sbin/multipathd";
my $UMOUNT = "/bin/umount";
my $VDO = "/bin/vdo";
my $sbindir = "/usr/sbin";
my $DMSETUP = "$sbindir/dmsetup";
my $LVM = "$sbindir/lvm";
my $FINDMNT_READ;
my $FINDMNT;
my $UMOUNT_OPTS;
if (!(# Original bash: "$UMOUNT" --help | grep -- "--all-targets" >"$DEV_DIR/null";
do {
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        my ($in_1, $out_1);
    my $pid_1 = open3($in_1, $out_1, '>&STDERR', 'unknown_command', '--help');
    close $in_1 or croak 'Close failed: $OS_ERROR';
    $output_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_1> };
    close $out_1 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_1, 0;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', "$DEV_DIR/null"
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_2 = q{};
    my $grep_result_3;
    my @grep_lines_3 = split /\n/msx, $output_0;
    my @grep_filtered_3 = grep { /--all-targets/msx } @grep_lines_3;
    $grep_result_3 = join "\n", @grep_filtered_3;
    if (!($grep_result_3 =~ m{\n\z} || $grep_result_3 eq q{})) {
    $grep_result_3 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_3 > 0 ? 0 : 1;
    $tmp_redirect_2 = $grep_result_3;
    $tmp_redirect_2;
    };
    print $tmp;
    if ($tmp eq q{}) { print $output_0; }
    $output_printed_0 = 1;
    open STDOUT, '>&', $original_stdout
    or die "Cannot restore STDOUT: $OS_ERROR\n";
    close $original_stdout
    or die "Close failed: $OS_ERROR\n";
    };
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    };)) {
    $UMOUNT_OPTS = "--all-targets ";
}
else {
    $UMOUNT_OPTS = "";
    $FINDMNT = "/bin/findmnt -r --noheadings -u -o TARGET";
    $FINDMNT_READ = "read -r mnt";
}
my $DMSETUP_OPTS = "";
my $LVM_OPTS = "";
my $MDADM_OPTS = "";
my $MPATHD_OPTS = "";
my $VDO_OPTS = "";
my $LSBLK = "/bin/lsblk -r --noheadings -o TYPE,KNAME,NAME,MOUNTPOINT";
my $LSBLK_VARS = "local devtype local kname local name local mnt";
my $LSBLK_READ = "read -r devtype kname name mnt";
my $SORT_MNT = "/bin/sort -r -u -k 4";
my $ERRORS = q{0};
my $VERBOSE = q{0};
my $DO_UMOUNT = q{0};
my $LVM_DO_WHOLE_VG = q{0};
my $LVM_CONFIG = "activation{retry_deactivation=0}";
my $MDRAID_DO_WAIT = q{0};
my $MPATHD_DO_DISABLEQUEUEING = q{0};
my %SKIP_DEVICE_LIST = ();
my %SKIP_VG_LIST = ();
my %SKIP_UMOUNT_LIST = ('[/]=1', '[/lib]=1', '[/lib64]=1', '[/bin]=1', '[/sbin]=1', '[/var]=1', '[/var/log]=1', '[/usr]=1', '[/usr/lib]=1', '[/usr/lib64]=1', '[/usr/sbin]=1', '[/usr/bin]=1');
$SKIP_UMOUNT_LIST{"[SWAP"} = q{1};

sub usage {
    say ${TOOL} . ": Utility to deactivate block devices";
    print "\n";
    say "  " . ${TOOL} . " [options] [device...]";
    say "    - Deactivate block device tree.";
    say "      If devices are specified, deactivate only supplied devices and their holders.";
    print "\n";
    say "  Options:";
    say "    -e | --errors                       Show errors reported from tools";
    say "    -h | --help                         Show this help message";
    say "    -d | --dmoptions     DM_OPTIONS     Comma separated DM specific options";
    say "    -l | --lvmoptions    LVM_OPTIONS    Comma separated LVM specific options";
    say "    -m | --mpathoptions  MPATH_OPTIONS  Comma separated DM-multipath specific options";
    say "    -r | --mdraidoptions MDRAID_OPTIONS Comma separated MD RAID specific options";
    say "    -o | --vdooptions    VDO_OPTIONS    Comma separated VDO specific options";
    say "    -u | --umount                       Unmount the device if mounted";
    say "    -v | --verbose                      Verbose mode (also implies -e)";
    print "\n";
    say "  Device specific options:";
    say "    DM_OPTIONS:";
    say "      retry           retry removal several times in case of failure";
    say "      force           force device removal";
    say "    LVM_OPTIONS:";
    say "      retry           retry removal several times in case of failure";
    say "      wholevg         deactivate the whole VG when processing an LV";
    say "    MDRAID_OPTIONS:";
    say "      wait            wait for resync, recovery or reshape to complete first";
    say "    MPATH_OPTIONS:";
    say "      disablequeueing disable queueing on all DM-multipath devices first";
    say "    VDO_OPTIONS:";
    say "      configfile=file use specified VDO configuration file";
exit $main_exit_code;
    return;
}

sub add_device_to_skip_list {
    push @SKIP_DEVICE_LIST, '[$kname]=1';
return q{1};
    return;
}

sub add_vg_to_skip_list {
    push @SKIP_VG_LIST, '[$DM_VG_NAME]=1';
return q{1};
    return;
}

sub is_top_level_device {
    my $files = "$SYS_BLK_DIR/$ENV{kname}/holders/" . q{ } . q{*};
    $main_exit_code = system('test', '-z', "$files") >> 8;
    return;
}

sub device_umount_one {
    if (do {
$main_exit_code = system('test', '-z', "$ENV{mnt}") >> 8;
        $CHILD_ERROR == 0
    }) {
        return q{0};
    }
if ((StringInterpolation(StringInterpolation { parts: [ParameterExpansion(ParameterExpansion { variable: "SKIP_UMOUNT_LIST[\"$mnt\"]", operator: None, is_mutable: true })] }, None) eq q{} && (StringInterpolation(StringInterpolation { parts: [Variable("DO_UMOUNT")] }, None) == StringInterpolation(StringInterpolation { parts: [Literal("1")] }, None)))) {
        print "  [UMOUNT]: unmounting $ENV{name} ($ENV{kname}) mounted on $ENV{mnt}... ";
if (!(do { my $eval_input = $UMOUNT . $UMOUNT_OPTS . q{} . $OUT . $ERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
            say "done";
}
        else {
            if (!(            $CHILD_ERROR = 0)) {
                say "skipping";
                add_device_to_skip_list();
}
            else {
                say "already unmounted";
            }
        }
}
    else {
        say "  [SKIP]: unmount of $ENV{name} ($ENV{kname}) mounted on $ENV{mnt}";
        add_device_to_skip_list();
    }
    return;
}

sub device_umount {
    if (do {
if (do {
if (do {
$main_exit_code = system('test', "$ENV{devtype}", q{!}, q{=}, "lvm") >> 8;
    $CHILD_ERROR == 0
}) {
        $main_exit_code = system('test', substr($ENV{kname}, 0, 3), q{!}, q{=}, "dm-") >> 8;
}
    $CHILD_ERROR == 0
}) {
        $main_exit_code = system('test', substr($ENV{kname}, 0, 2), q{!}, q{=}, "md") >> 8;
}
        $CHILD_ERROR == 0
    }) {
        return q{0};
    }
if (StringInterpolation(StringInterpolation { parts: [Variable("FINDMNT")] }, None) eq q{}) {
        device_umount_one();
}
    else {
while (         $CHILD_ERROR = 0 ) {
                        device_umount_one();
            if ($CHILD_ERROR != 0) {
                return q{1};
            }
;
        }
    }
    return;
}

sub deactivate_holders {
    my $skip = "1";
    $CHILD_ERROR = 0;
while (     $CHILD_ERROR = 0 ) {
                $main_exit_code = system('test', '-e', "$SYS_BLK_DIR/$ENV{kname}") >> 8;
        if ($CHILD_ERROR != 0) {
            next;
        }
;
                $main_exit_code = system('test', '-z', $SKIP_DEVICE_LIST{'"$kname"'}) >> 8;
        if ($CHILD_ERROR != 0) {
            return q{1};
        }
;
        if (do {
if (do {
$main_exit_code = system('test', "$skip", '-eq', q{1}) >> 8;
    $CHILD_ERROR == 0
}) {
        $skip = q{0};
}
            $CHILD_ERROR == 0
        }) {
            next;
        }
                $main_exit_code = system('bash', 'deactivate') >> 8;
        if ($CHILD_ERROR != 0) {
            return q{1};
        }
;
    }
    return;
}

sub deactivate_dm {
    my $xname;
    $xname = sprintf('%s', "$ENV{name}");
        $main_exit_code = system('test', '-b', "$DEV_DIR/mapper/$xname") >> 8;
    if ($CHILD_ERROR != 0) {
        return q{0};
    }
;
        $main_exit_code = system('test', '-z', $SKIP_DEVICE_LIST{'"$kname"'}) >> 8;
    if ($CHILD_ERROR != 0) {
        return q{1};
    }
;
        deactivate_holders("$DEV_DIR/mapper/$xname");
    if ($CHILD_ERROR != 0) {
        return q{1};
    }
;
    print "  [DM]: deactivating $ENV{devtype} device $xname ($ENV{kname})... ";
if (!(do { my $eval_input = $DMSETUP . $DMSETUP_OPTS . "remove" . $xname . $OUT . $ERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
        say "done";
}
    else {
        say "skipping";
        add_device_to_skip_list();
    }
    return;
}

sub deactivate_lvm {
    my $DM_VG_NAME;
    my $DM_LV_NAME;
do { my $eval_input = q{}; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
        $main_exit_code = system('test', '-b', "$DEV_DIR/$DM_VG_NAME/$DM_LV_NAME") >> 8;
    if ($CHILD_ERROR != 0) {
        return q{0};
    }
;
        $main_exit_code = system('test', '-z', $SKIP_VG_LIST{'"$DM_VG_NAME"'}) >> 8;
    if ($CHILD_ERROR != 0) {
        return q{1};
    }
;
    my $lv_list;
if ((StringInterpolation(StringInterpolation { parts: [Variable("LVM_DO_WHOLE_VG")] }, None) == 0)) {
        if (do {
$main_exit_code = system('test', "$ENV{LVM_AVAILABLE}", '-eq', q{0}) >> 8;
            $CHILD_ERROR == 0
        }) {
                            add_device_to_skip_list();
return q{1};
        }
                deactivate_holders("$DEV_DIR/$DM_VG_NAME/$DM_LV_NAME");
        if ($CHILD_ERROR != 0) {
                            add_device_to_skip_list();
return q{1};
        }
;
        print "  [LVM]: deactivating Logical Volume $DM_VG_NAME/$DM_LV_NAME... ";
if (!(do { my $eval_input = $LVM . "lvchange" . $LVM_OPTS . "--config" . "\\'log\\{prefix" . "=" . "\\\"\\\"\\}" . $LVM_CONFIG . "\\'" . "-aln" . $DM_VG_NAME . "/" . $DM_LV_NAME . $OUT . $ERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
            say "done";
}
        else {
            say "skipping";
            add_device_to_skip_list();
        }
}
    else {
        if (do {
$main_exit_code = system('test', "$ENV{LVM_AVAILABLE}", '-eq', q{0}) >> 8;
            $CHILD_ERROR == 0
        }) {
                            add_vg_to_skip_list();
return q{1};
        }
        $lv_list = do {
    local $ENV{DM_VG_NAME} = $DM_VG_NAME;
    local $ENV{ERR} = $ERR;
    local $ENV{LVM} = $LVM;
    local $ENV{LVM_CONFIG} = $LVM_CONFIG;
    my $command = q{: 'Complex command not supported in bash string generation'};
    my ($in, $out, $err);
    my $pid = open3($in, $out, $err, 'bash', '-c', $command);
    close $in or croak 'Close failed: $OS_ERROR';
    my $result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out> };
    close $out or croak 'Close failed: $OS_ERROR';
    waitpid $pid, 0;
    $CHILD_ERROR = $? >> 8;
    $result;
};
        my $lv;
        for my $lv ($lv_list) {
                        $main_exit_code = system('test', '-b', "$DEV_DIR/$DM_VG_NAME/$lv") >> 8;
            if ($CHILD_ERROR != 0) {
                next;
            }
;
                        deactivate_holders("$DEV_DIR/$DM_VG_NAME/$lv");
            if ($CHILD_ERROR != 0) {
                                    add_vg_to_skip_list();
return q{1};
            }
;
        }
;
        print "  [LVM]: deactivating Volume Group $DM_VG_NAME... ";
if (!(do { my $eval_input = $LVM . "vgchange" . $LVM_OPTS . "--config" . "\\'log\\{prefix" . "=" . "\\\"" . "\\\"\\}" . $LVM_CONFIG . "\\'" . "-aln" . $DM_VG_NAME . $OUT . $ERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
            say "done";
}
        else {
            say "skipping";
            add_vg_to_skip_list();
        }
    }
;
    return;
}

sub deactivate_md {
    my $xname;
    $xname = sprintf('%s', "$ENV{name}");
    my $sync_action;
        $main_exit_code = system('test', '-b', "$DEV_DIR/$xname") >> 8;
    if ($CHILD_ERROR != 0) {
        return q{0};
    }
;
        $main_exit_code = system('test', '-z', $SKIP_DEVICE_LIST{'"$kname"'}) >> 8;
    if ($CHILD_ERROR != 0) {
        return q{1};
    }
;
    if (do {
$main_exit_code = system('test', "$ENV{MDADM_AVAILABLE}", '-eq', q{0}) >> 8;
        $CHILD_ERROR == 0
    }) {
                    add_device_to_skip_list();
return q{1};
    }
        deactivate_holders("$DEV_DIR/$xname");
    if ($CHILD_ERROR != 0) {
        return q{1};
    }
;
    print "  [MD]: deactivating $ENV{devtype} device $ENV{kname}... ";
    if (do {
$main_exit_code = system('test', "$MDRAID_DO_WAIT", '-eq', q{1}) >> 8;
        $CHILD_ERROR == 0
    }) {
                    $sync_action = do { my $cat_chunk = q{}; if ( open my $fh, '<', "$SYS_BLK_DIR/$ENV{kname}/md/sync_action" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "$SYS_BLK_DIR/$ENV{kname}/md/sync_action" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
            if (do {
$main_exit_code = system('test', "$sync_action", q{!}, q{=}, "idle") >> 8;
                $CHILD_ERROR == 0
            }) {
                                    print "$sync_action action in progress... ";
if (!(do { my $eval_input = $MDADM . $MDADM_OPTS . "-W" . $DEV_DIR . "/" . $kname . $OUT . $ERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
                        print "complete... ";
}
                    else {
                        if (do {
$main_exit_code = system('test', $?, '-ne', q{1}) >> 8;
                            $CHILD_ERROR == 0
                        }) {
                                                        print "failed to wait for $sync_action action... ";
                        }
                    }
            }
    }
if (!(do { my $eval_input = $MDADM . $MDADM_OPTS . "-S" . $xname . $OUT . $ERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
        say "done";
}
    else {
        say "skipping";
        add_device_to_skip_list();
    }
    return;
}

sub deactivate_vdo {
    my $xname;
    $xname = sprintf('%s', "$ENV{name}");
        $main_exit_code = system('test', '-b', "$DEV_DIR/mapper/$xname") >> 8;
    if ($CHILD_ERROR != 0) {
        return q{0};
    }
;
        $main_exit_code = system('test', '-z', $SKIP_DEVICE_LIST{'"$kname"'}) >> 8;
    if ($CHILD_ERROR != 0) {
        return q{1};
    }
;
    if (do {
$main_exit_code = system('test', "$ENV{VDO_AVAILABLE}", '-eq', q{0}) >> 8;
        $CHILD_ERROR == 0
    }) {
                    add_device_to_skip_list();
return q{1};
    }
        deactivate_holders("$DEV_DIR/mapper/$xname");
    if ($CHILD_ERROR != 0) {
        return q{1};
    }
;
    print "  [VDO]: deactivating VDO volume $xname... ";
if (!(do { my $eval_input = $VDO . "stop" . $VDO_OPTS . "--name=$xname" . $OUT . $ERR; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };)) {
        say "done";
}
    else {
        say "skipping";
        add_device_to_skip_list();
    }
    return;
}

sub deactivate {
if (StringInterpolation(StringInterpolation { parts: [Variable("devtype")] }, None) eq StringInterpolation(StringInterpolation { parts: [Literal("lvm")] }, None)) {
        deactivate_lvm();
}
    else {
        if (StringInterpolation(StringInterpolation { parts: [Variable("devtype")] }, None) eq StringInterpolation(StringInterpolation { parts: [Literal("vdo")] }, None)) {
            deactivate_vdo();
}
        else {
            if (StringInterpolation(StringInterpolation { parts: [ParameterExpansion(ParameterExpansion { variable: "kname:0:3", operator: None, is_mutable: true })] }, None) eq StringInterpolation(StringInterpolation { parts: [Literal("dm-")] }, None)) {
                deactivate_dm();
}
            else {
                if (StringInterpolation(StringInterpolation { parts: [ParameterExpansion(ParameterExpansion { variable: "kname:0:2", operator: None, is_mutable: true })] }, None) eq StringInterpolation(StringInterpolation { parts: [Literal("md")] }, None)) {
                    deactivate_md();
                }
            }
        }
    }
    return;
}

sub deactivate_all {
    my ($file) = @_;
    $CHILD_ERROR = 0;
    my $skip = q{0};
    say "Deactivating block devices:";
    if (do {
$main_exit_code = system('test', "$ENV{MPATHD_RUNNING}", '-eq', q{1}) >> 8;
        $CHILD_ERROR == 0
    }) {
                    print "  [DM]: disabling queueing on all multipath devices... ";
                        if (do {
do {
    my $output_6 = q{};
    my $output_printed_6;
    my $pipeline_success_6 = 1;
        my @_pcmd_8 = ('bash', '-c', ": \"Complex command cannot be converted to shell command\"");
    my ($in_7);
    my $pid_7 = open3($in_7, $out_7, '>&STDERR', @_pcmd_8);
    close $in_7 or croak 'Close failed: $OS_ERROR';
    my $temp_result;
    $temp_result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_7> };
    $output_6 = $temp_result;
    close $out_7 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_7, 0;

        do {
    open my $original_stdout, '>&', STDOUT
    or die "Cannot save STDOUT: $OS_ERROR\n";
    open STDOUT, '>', "$DEV_DIR/null"
    or die "Cannot access file: $OS_ERROR\n";
    my $tmp = do {
    my $tmp_redirect_9 = q{};
    my $grep_result_10;
    my @grep_lines_10 = split /\n/msx, $output_6;
    my @grep_filtered_10 = grep { /^ok$/msx } @grep_lines_10;
    $grep_result_10 = join "\n", @grep_filtered_10;
    if (!($grep_result_10 =~ m{\n\z} || $grep_result_10 eq q{})) {
    $grep_result_10 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_10 > 0 ? 0 : 1;
    $tmp_redirect_9 = $grep_result_10;
    $tmp_redirect_9;
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
    }
                $CHILD_ERROR == 0
            }) {
                                say "done";
            }
            if ($CHILD_ERROR != 0) {
                                say "failed";
            }
    }
if ((Variable("#", false, None) == 0)) {
while (         $CHILD_ERROR = 0 ) {
            device_umount();
        }
while (         $CHILD_ERROR = 0 ) {
            if (do {
$main_exit_code = system('test', "$ENV{devtype}", q{=}, "disk") >> 8;
                $CHILD_ERROR == 0
            }) {
                next;
            }
            if (do {
$main_exit_code = system('test', "$skip", '-eq', q{1}) >> 8;
                $CHILD_ERROR == 0
            }) {
                if (!(                    is_top_level_device())) {
                        $skip = q{0};
}
                    else {
next;
                    }
            }
                        $main_exit_code = system('test', '-z', $SKIP_DEVICE_LIST{'"$kname"'}) >> 8;
            if ($CHILD_ERROR != 0) {
                next;
            }
;
                        deactivate();
            if ($CHILD_ERROR != 0) {
                                $skip = q{1};
            }
;
        }
}
    else {
        my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
while (             $CHILD_ERROR = 0 ) {
                device_umount();
            }
if ((-b StringInterpolation(StringInterpolation { parts: [Variable("1")] }, None))) {
                $CHILD_ERROR = 0;
                                $main_exit_code = system('test', '-z', $SKIP_DEVICE_LIST{'"$kname"'}) >> 8;
                if ($CHILD_ERROR != 0) {
                    # Builtin command 'shift' not implemented
next;
                }
;
                deactivate();
}
            else {
                say "$_[0]: device not found";
return q{1};
            }
# Builtin command 'shift' not implemented
        }
;
    }
    return;
}

sub get_dmopts {
    my $ORIG_IFS = $IFS;
    my $IFS = q{,};
    my $opt;
    for my $opt ($1) {
if ($opt eq '') {
        } elsif ($opt eq 'retry') {
                        $DMSETUP_OPTS = "--retry ";
        } elsif ($opt eq 'force') {
                        $DMSETUP_OPTS = "--force ";
        } elsif (1) {
                        say "$opt: unknown DM option";
        }
    }
;
    $IFS = $ORIG_IFS;
    return;
}

sub get_lvmopts {
    my $ORIG_IFS = $IFS;
    my $IFS = q{,};
    my $opt;
    for my $opt ($1) {
if ("$opt" eq '') {
        } elsif ("$opt" eq 'retry') {
                        $LVM_CONFIG = "activation{retry_deactivation=1}";
        } elsif ("$opt" eq 'wholevg') {
                        $LVM_DO_WHOLE_VG = q{1};
        } elsif (1) {
                        say "$opt: unknown LVM option";
        }
    }
;
    $IFS = $ORIG_IFS;
    return;
}

sub get_mdraidopts {
    my $ORIG_IFS = $IFS;
    my $IFS = q{,};
    my $opt;
    for my $opt ($1) {
if ("$opt" eq '') {
        } elsif ("$opt" eq 'wait') {
                        $MDRAID_DO_WAIT = q{1};
        } elsif (1) {
                        say "$opt: unknown MD RAID option";
        }
    }
;
    $IFS = $ORIG_IFS;
    return;
}

sub get_mpathopts {
    my $ORIG_IFS = $IFS;
    my $IFS = q{,};
    my $opt;
    for my $opt ($1) {
if ("$opt" eq '') {
        } elsif ("$opt" eq 'disablequeueing') {
                        $MPATHD_DO_DISABLEQUEUEING = q{1};
        } elsif (1) {
                        say "$opt: unknown DM-multipath option";
        }
    }
;
    $IFS = $ORIG_IFS;
    return;
}

sub get_vdoopts {
    my $ORIG_IFS = $IFS;
    my $IFS = q{,};
    my $tmp;
    my $opt;
    for my $opt ($1) {
if ("$opt" eq '') {
        } elsif ("$opt" =~ /^configfile=.*$/msx) {
                        $tmp = ${opt} =~ s/^.*?=//r;
                        $VDO_OPTS = "--confFile=" . (${tmp} =~ s/,.*$//sr =~ s/,.*$//sr) . " ";
        } elsif (1) {
                        say "$opt: unknown VDO option";
        }
    }
;
    $IFS = $ORIG_IFS;
    return;
}

sub set_env {
    my $ERR;
if ((StringInterpolation(StringInterpolation { parts: [Variable("ERRORS")] }, None) == StringInterpolation(StringInterpolation { parts: [Literal("1")] }, None))) {
undef $ERR;
delete $ENV{ERR};
}
    else {
        $ERR = "2>$DEV_DIR/null";
    }
;
    my $OUT;
if ((StringInterpolation(StringInterpolation { parts: [Variable("VERBOSE")] }, None) == StringInterpolation(StringInterpolation { parts: [Literal("1")] }, None))) {
undef $OUT;
delete $ENV{OUT};
        $UMOUNT_OPTS = "-v";
        $DMSETUP_OPTS = "-vvvv";
        $LVM_OPTS = "-vvvv";
        $MDADM_OPTS = "-vv";
        $MPATHD_OPTS = "-v 3";
        $VDO_OPTS = "--verbose ";
}
    else {
        $OUT = "1>$DEV_DIR/null";
    }
;
    my $LVM_AVAILABLE;
if ((-f 'StringInterpolation(StringInterpolation { parts: [Variable("LVM")] }, None)')) {
        $LVM_AVAILABLE = q{1};
}
    else {
        $LVM_AVAILABLE = q{0};
    }
;
    my $MDADM_AVAILABLE;
if ((-f 'Variable("MDADM", false, None)')) {
        $MDADM_AVAILABLE = q{1};
}
    else {
        $MDADM_AVAILABLE = q{0};
    }
;
    my $VDO_AVAILABLE;
if ((-f 'Variable("VDO", false, None)')) {
        $VDO_AVAILABLE = q{1};
}
    else {
        $VDO_AVAILABLE = q{0};
    }
;
    my $MPATHD_RUNNING = q{0};
    if (do {
$main_exit_code = system('test', "$MPATHD_DO_DISABLEQUEUEING", '-eq', q{1}) >> 8;
        $CHILD_ERROR == 0
    }) {
        if ((-f 'StringInterpolation(StringInterpolation { parts: [Variable("MPATHD")] }, None)')) {
if (!(                # Original bash: eval "$MPATHD" show daemon "$ERR" | grep "running" >"$DEV_DIR/null";
do {
                    my $output_15 = q{};
                    my $output_printed_15;
                    my $pipeline_success_15 = 1;
                                        my @_pcmd_17 = ('bash', '-c', ": \"Complex command cannot be converted to shell command\"");
                    my ($in_16);
                    my $pid_16 = open3($in_16, $out_16, '>&STDERR', @_pcmd_17);
                    close $in_16 or croak 'Close failed: $OS_ERROR';
                    my $temp_result;
                    $temp_result = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_16> };
                    $output_15 = $temp_result;
                    close $out_16 or croak 'Close failed: $OS_ERROR';
                    waitpid $pid_16, 0;

                                        do {
                    open my $original_stdout, '>&', STDOUT
                    or die "Cannot save STDOUT: $OS_ERROR\n";
                    open STDOUT, '>', "$DEV_DIR/null"
                    or die "Cannot access file: $OS_ERROR\n";
                    my $tmp = do {
                    my $tmp_redirect_18 = q{};
                    my $grep_result_19;
                    my @grep_lines_19 = split /\n/msx, $output_15;
                    my @grep_filtered_19 = grep { /running/msx } @grep_lines_19;
                    $grep_result_19 = join "\n", @grep_filtered_19;
                    if (!($grep_result_19 =~ m{\n\z} || $grep_result_19 eq q{})) {
                    $grep_result_19 .= "\n";
                    }
                    $CHILD_ERROR = scalar @grep_filtered_19 > 0 ? 0 : 1;
                    $tmp_redirect_18 = $grep_result_19;
                    $tmp_redirect_18;
                    };
                    print $tmp;
                    if ($tmp eq q{}) { print $output_15; }
                    $output_printed_15 = 1;
                    open STDOUT, '>&', $original_stdout
                    or die "Cannot restore STDOUT: $OS_ERROR\n";
                    close $original_stdout
                    or die "Close failed: $OS_ERROR\n";
                    };
                    if ( !$pipeline_success_15 ) { $main_exit_code = 1; }
                    };)) {
                    $MPATHD_RUNNING = q{1};
                }
            }
    }
    return;
}
my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '') {
    } elsif ("$_[0]" eq '-e' or "$_[0]" eq '--errors') {
                $ERRORS = q{1};
    } elsif ("$_[0]" eq '-h' or "$_[0]" eq '--help') {
                usage();
    } elsif ("$_[0]" eq '-d' or "$_[0]" eq '--dmoptions') {
                get_dmopts("$_[1]");
        # Builtin command 'shift' not implemented
    } elsif ("$_[0]" eq '-l' or "$_[0]" eq '--lvmoptions') {
                get_lvmopts("$_[1]");
        # Builtin command 'shift' not implemented
    } elsif ("$_[0]" eq '-m' or "$_[0]" eq '--mpathoptions') {
                get_mpathopts("$_[1]");
        # Builtin command 'shift' not implemented
    } elsif ("$_[0]" eq '-r' or "$_[0]" eq '--mdraidoptions') {
                get_mdraidopts("$_[1]");
        # Builtin command 'shift' not implemented
    } elsif ("$_[0]" eq '-o' or "$_[0]" eq '--vdooptions') {
                get_vdoopts("$_[1]");
        # Builtin command 'shift' not implemented
    } elsif ("$_[0]" eq '-u' or "$_[0]" eq '--umount') {
                $DO_UMOUNT = q{1};
    } elsif ("$_[0]" eq '-v' or "$_[0]" eq '--verbose') {
                $VERBOSE = q{1};
                $ERRORS = q{1};
    } elsif ("$_[0]" eq '-vv') {
                $VERBOSE = q{1};
                $ERRORS = q{1};
        # set -x not implemented
    } elsif (1) {
        last;    }
# Builtin command 'shift' not implemented
}
set_env();
deactivate_all("\@ARGV");

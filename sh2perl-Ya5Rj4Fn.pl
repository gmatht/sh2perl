#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $dashless = do { my $result_0 = qx{bash -c q{basename "$0" | sed -e 's/-/ /'} }; chomp $result_0; $result_0; };
my $USAGE = "[--quiet] [--cached]
   or: $dashless [--quiet] add [-b <branch>] [-f|--force] [--name <name>] [--reference <repository>] [--] <repository> [<path>]
   or: $dashless [--quiet] status [--cached] [--recursive] [--] [<path>...]
   or: $dashless [--quiet] init [--] [<path>...]
   or: $dashless [--quiet] deinit [-f|--force] (--all| [--] <path>...)
   or: $dashless [--quiet] update [--init [--filter=<filter-spec>]] [--remote] [-N|--no-fetch] [-f|--force] [--checkout|--merge|--rebase] [--[no-]recommend-shallow] [--reference <repository>] [--recursive] [--[no-]single-branch] [--] [<path>...]
   or: $dashless [--quiet] set-branch (--default|--branch <branch>) [--] <path>
   or: $dashless [--quiet] set-url [--] <path> <newurl>
   or: $dashless [--quiet] summary [--cached|--files] [--summary-limit <n>] [commit] [--] [<path>...]
   or: $dashless [--quiet] foreach [--recursive] <command>
   or: $dashless [--quiet] sync [--recursive] [--] [<path>...]
   or: $dashless [--quiet] absorbgitdirs [--] [<path>...]";
my $OPTIONS_SPEC = q{};
my $SUBDIRECTORY_OK = 'Yes';
$main_exit_code = system('.', 'git-sh-setup') >> 8;
$main_exit_code = system('bash', 'require_work_tree') >> 8;
my $wt_prefix = do {
    my ($in_1, $out_1);
    my $pid_1 = open3($in_1, $out_1, '>&STDERR', 'git', 'rev-parse', '--show-prefix');
    close $in_1 or croak 'Close failed: $OS_ERROR';
    my $result_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_1> };
    close $out_1 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_1, 0;
    $result_1
};
$main_exit_code = system('bash', 'cd_to_toplevel') >> 8;
my $GIT_PROTOCOL_FROM_USER = q{0};
$ENV{GIT_PROTOCOL_FROM_USER} = $GIT_PROTOCOL_FROM_USER;
my $command = q{};
my $quiet = q{};
my $branch = q{};
my $force = q{};
my $reference = q{};
my $cached = q{};
my $recursive = q{};
my $init = q{};
my $require_init = q{};
my $files = q{};
my $remote = q{};
my $nofetch = q{};
my $rebase = q{};
my $merge = q{};
my $checkout = q{};
my $custom_name = q{};
my $depth = q{};
my $progress = q{};
my $dissociate = q{};
my $single_branch = q{};
my $jobs = q{};
my $recommend_shallow = q{};
my $filter = q{};

sub isnumber {
    my ($file) = @_;
    if (do {
                do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
            my $n = eval { int($_[0] + 0) } // "";
        };
    } == 0) {
                $main_exit_code = system('test', "$n", q{=}, "$_[0]") >> 8;
    }
    return;
}

sub cmd_add {
    my ($file) = @_;
    my $reference_path = q{};
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-b' or "$_[0]" eq '--branch') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $branch = $_[1];
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" eq '-f' or "$_[0]" eq '--force') {
                        $force = $_[0];
        } elsif ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
        } elsif ("$_[0]" eq '--progress') {
                        $progress = q{1};
        } elsif ("$_[0]" eq '--reference') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $reference_path = $_[1];
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" =~ /^--reference=.*$/msx) {
                        $reference_path = ($_[0] =~ s/^--reference=//r =~ s/^--reference=//r);
        } elsif ("$_[0]" eq '--dissociate') {
                        $dissociate = q{1};
        } elsif ("$_[0]" eq '--name') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $custom_name = $_[1];
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" eq '--depth') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $depth = "--depth=$_[1]";
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" =~ /^--depth=.*$/msx) {
                        $depth = $_[0];
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
if (StringInterpolation(StringInterpolation { parts: [Variable("1")] }, None) eq q{}) {
        $main_exit_code = system('bash', 'usage') >> 8;
    }
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'add', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), (defined ${force} && ${force} ne q{} ? ${force} : '--force'), (defined ${progress} && ${progress} ne q{} ? ${progress} : '"--progress"'), (defined ${branch} && ${branch} ne q{} ? ${branch} : '--branch "$branch"'), (defined ${reference_path} && ${reference_path} ne q{} ? ${reference_path} : '--reference "$reference_path"'), (defined ${dissociate} && ${dissociate} ne q{} ? ${dissociate} : '--dissociate'), (defined ${custom_name} && ${custom_name} ne q{} ? ${custom_name} : '--name "$custom_name"'), (defined ${depth} && ${depth} ne q{} ? ${depth} : '"$depth"'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_foreach {
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
        } elsif ("$_[0]" eq '--recursive') {
                        $recursive = q{1};
        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'foreach', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), (defined ${recursive} && ${recursive} ne q{} ? ${recursive} : '--recursive'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_init {
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'init', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_deinit {
    my ($file) = @_;
    my $deinit_all = q{};
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-f' or "$_[0]" eq '--force') {
                        $force = $_[0];
        } elsif ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
        } elsif ("$_[0]" eq '--all') {
                        $deinit_all = q{t};
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'deinit', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), (defined ${force} && ${force} ne q{} ? ${force} : '--force'), (defined ${deinit_all} && ${deinit_all} ne q{} ? ${deinit_all} : '--all'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_update {
    my ($file) = @_;
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
        } elsif ("$_[0]" eq '-v' or "$_[0]" eq '--verbose') {
                        $quiet = q{0};
        } elsif ("$_[0]" eq '--progress') {
                        $progress = q{1};
        } elsif ("$_[0]" eq '-i' or "$_[0]" eq '--init') {
                        $init = q{1};
        } elsif ("$_[0]" eq '--require-init') {
                        $require_init = q{1};
        } elsif ("$_[0]" eq '--remote') {
                        $remote = q{1};
        } elsif ("$_[0]" eq '-N' or "$_[0]" eq '--no-fetch') {
                        $nofetch = q{1};
        } elsif ("$_[0]" eq '-f' or "$_[0]" eq '--force') {
                        $force = $_[0];
        } elsif ("$_[0]" eq '-r' or "$_[0]" eq '--rebase') {
                        $rebase = q{1};
        } elsif ("$_[0]" eq '--reference') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $reference = "--reference=$_[1]";
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" =~ /^--reference=.*$/msx) {
                        $reference = "$_[0]";
        } elsif ("$_[0]" eq '--dissociate') {
                        $dissociate = q{1};
        } elsif ("$_[0]" eq '-m' or "$_[0]" eq '--merge') {
                        $merge = q{1};
        } elsif ("$_[0]" eq '--recursive') {
                        $recursive = q{1};
        } elsif ("$_[0]" eq '--checkout') {
                        $checkout = q{1};
        } elsif ("$_[0]" eq '--recommend-shallow') {
                        $recommend_shallow = "--recommend-shallow";
        } elsif ("$_[0]" eq '--no-recommend-shallow') {
                        $recommend_shallow = "--no-recommend-shallow";
        } elsif ("$_[0]" eq '--depth') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $depth = "--depth=$_[1]";
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" =~ /^--depth=.*$/msx) {
                        $depth = $_[0];
        } elsif ("$_[0]" eq '-j' or "$_[0]" eq '--jobs') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $jobs = "--jobs=$_[1]";
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" =~ /^--jobs=.*$/msx) {
                        $jobs = $_[0];
        } elsif ("$_[0]" eq '--single-branch') {
                        $single_branch = "--single-branch";
        } elsif ("$_[0]" eq '--no-single-branch') {
                        $single_branch = "--no-single-branch";
        } elsif ("$_[0]" eq '--filter') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $filter = "--filter=$_[1]";
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" =~ /^--filter=.*$/msx) {
                        $filter = "$_[0]";
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'update', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), (defined ${force} && ${force} ne q{} ? ${force} : '--force'), (defined ${progress} && ${progress} ne q{} ? ${progress} : '"--progress"'), (defined ${remote} && ${remote} ne q{} ? ${remote} : '--remote'), (defined ${recursive} && ${recursive} ne q{} ? ${recursive} : '--recursive'), (defined ${init} && ${init} ne q{} ? ${init} : '--init'), (defined ${nofetch} && ${nofetch} ne q{} ? ${nofetch} : '--no-fetch'), (defined ${rebase} && ${rebase} ne q{} ? ${rebase} : '--rebase'), (defined ${merge} && ${merge} ne q{} ? ${merge} : '--merge'), (defined ${checkout} && ${checkout} ne q{} ? ${checkout} : '--checkout'), (defined ${reference} && ${reference} ne q{} ? ${reference} : '"$reference"'), (defined ${dissociate} && ${dissociate} ne q{} ? ${dissociate} : '"--dissociate"'), (defined ${depth} && ${depth} ne q{} ? ${depth} : '"$depth"'), (defined ${require_init} && ${require_init} ne q{} ? ${require_init} : '--require-init'), (defined ${dissociate} && ${dissociate} ne q{} ? ${dissociate} : '"--dissociate"'), $single_branch, $recommend_shallow, $jobs, $filter, '--', "\@ARGV") >> 8;
    return;
}

sub cmd_set_branch {
    my ($file) = @_;
    my $default = q{};
    $branch = q{};
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
        } elsif ("$_[0]" eq '-d' or "$_[0]" eq '--default') {
                        $default = q{1};
        } elsif ("$_[0]" eq '-b' or "$_[0]" eq '--branch') {
            if ("$_[1]" eq '') {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
                        $branch = $_[1];
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'set-branch', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), (defined ${branch} && ${branch} ne q{} ? ${branch} : '--branch "$branch"'), (defined ${default} && ${default} ne q{} ? ${default} : '--default'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_set_url {
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'set-url', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_summary {
    my ($file) = @_;
    my $summary_limit = '-1';
    my $for_status = q{};
    my $diff_cmd = 'diff-index';
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '--cached') {
                        $cached = q{1};
        } elsif ("$_[0]" eq '--files') {
                        $files = "$_[0]";
        } elsif ("$_[0]" eq '--for-status') {
                        $for_status = "$_[0]";
        } elsif ("$_[0]" eq '-n' or "$_[0]" eq '--summary-limit') {
                        $summary_limit = "$_[1]";
                                    isnumber("$summary_limit");
            if ($CHILD_ERROR != 0) {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" =~ /^--summary-limit=.*$/msx) {
                        $summary_limit = ($_[0] =~ s/^--summary-limit=//r =~ s/^--summary-limit=//r);
                                    isnumber("$summary_limit");
            if ($CHILD_ERROR != 0) {
                                $main_exit_code = system('bash', 'usage') >> 8;
            }
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'summary', (defined ${files} && ${files} ne q{} ? ${files} : '--files'), (defined ${cached} && ${cached} ne q{} ? ${cached} : '--cached'), (defined ${for_status} && ${for_status} ne q{} ? ${for_status} : '--for-status'), (defined ${summary_limit} && ${summary_limit} ne q{} ? ${summary_limit} : '-n $summary_limit'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_status {
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
        } elsif ("$_[0]" eq '--cached') {
                        $cached = q{1};
        } elsif ("$_[0]" eq '--recursive') {
                        $recursive = q{1};
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
# Builtin command 'shift' not implemented
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'status', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), (defined ${cached} && ${cached} ne q{} ? ${cached} : '--cached'), (defined ${recursive} && ${recursive} ne q{} ? ${recursive} : '--recursive'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_sync {
    my $# = 0;
while ( (Variable("#", false, None) != 0) ) {
if ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                        $quiet = q{1};
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" eq '--recursive') {
                        $recursive = q{1};
            # Builtin command 'shift' not implemented
        } elsif ("$_[0]" eq '--') {
            # Builtin command 'shift' not implemented
            last;        } elsif ("$_[0]" =~ /^-.*$/msx) {
                        $main_exit_code = system('bash', 'usage') >> 8;
        } elsif (1) {
            last;        }
    }
;
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'sync', (defined ${quiet} && ${quiet} ne q{} ? ${quiet} : '--quiet'), (defined ${recursive} && ${recursive} ne q{} ? ${recursive} : '--recursive'), '--', "\@ARGV") >> 8;
    return;
}

sub cmd_absorbgitdirs {
    $main_exit_code = system('git', (defined ${wt_prefix} && ${wt_prefix} ne q{} ? ${wt_prefix} : '-C "$wt_prefix"'), 'submodule--helper', 'absorbgitdirs', "\@ARGV") >> 8;
    return;
}
while (1) {
    last unless do {
        $main_exit_code = system('test', scalar(@ARGV), q{!}, q{=}, q{0}) >> 8;
        $CHILD_ERROR == 0
    };
    last unless do {
        $main_exit_code = system('test', '-z', "$command") >> 8;
        $CHILD_ERROR == 0
    };
if ("$_[0]" eq 'add' or "$_[0]" eq 'foreach' or "$_[0]" eq 'init' or "$_[0]" eq 'deinit' or "$_[0]" eq 'update' or "$_[0]" eq 'set-branch' or "$_[0]" eq 'set-url' or "$_[0]" eq 'status' or "$_[0]" eq 'summary' or "$_[0]" eq 'sync' or "$_[0]" eq 'absorbgitdirs') {
                $command = $1;
    } elsif ("$_[0]" eq '-q' or "$_[0]" eq '--quiet') {
                $quiet = q{1};
    } elsif ("$_[0]" eq '--cached') {
                $cached = q{1};
    } elsif ("$_[0]" eq '--') {
        last;    } elsif ("$_[0]" =~ /^-.*$/msx) {
                $main_exit_code = system('bash', 'usage') >> 8;
    } elsif (1) {
        last;    }
# Builtin command 'shift' not implemented
}
if (StringInterpolation(StringInterpolation { parts: [Variable("command")] }, None) eq q{}) {
if (Variable("#", false, None) eq 0) {
        $command = 'status';
}
    else {
        $main_exit_code = system('bash', 'usage') >> 8;
    }
}
if (((!(system('test', '-n', "$cached") >> 8) && !(system('test', "$command", q{!}, q{=}, 'status') >> 8)) && !(system('test', "$command", q{!}, q{=}, 'summary') >> 8))) {
    $main_exit_code = system('bash', 'usage') >> 8;
}
$CHILD_ERROR = 0;

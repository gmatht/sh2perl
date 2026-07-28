#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $__docker_previous_extglob_setting = ('shopt -p extglob');
# extglob option enabled

sub __docker_q {
    do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
        $main_exit_code = system('docker', (defined ($ENV{host} // q{}) && ($ENV{host} // q{}) ne q{} ? ($ENV{host} // q{}) : '--host "$host"'), (defined ($ENV{config} // q{}) && ($ENV{config} // q{}) ne q{} ? ($ENV{config} // q{}) : '--config "$config"'), (defined ($ENV{context} // q{}) && ($ENV{context} // q{}) ne q{} ? ($ENV{context} // q{}) : '--context "$context"'), "\@ARGV") >> 8;
    };
    return;
}

sub __docker_configs {
    my $format;
if ("${1-}" eq "--id") {
        $format = '{{.ID}}';
# Builtin command 'shift' not implemented
}
    else {
        if ("${1-}" eq "--name") {
            $format = '{{.Name}}';
# Builtin command 'shift' not implemented
}
        else {
            if ("${DOCKER_COMPLETION_SHOW_CONFIG_IDS-}" eq yes) {
                $format = '{{.ID}} {{.Name}}';
}
            else {
                $format = '{{.Name}}';
            }
        }
    }
    __docker_q('config', 'ls', '--format', "$format", "\@ARGV");
    return;
}

sub __docker_complete_configs {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("$_[0]" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_1, $out_1);
    my $pid_1 = open3($in_1, $out_1, '>&STDERR', 'compgen', '-W', (do {
    my ($in_0, $out_0);
    my $pid_0 = open3($in_0, $out_0, '>&STDERR', '__docker_configs', "\@ARGV");
    close $in_0 or croak 'Close failed: $OS_ERROR';
    my $result_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_0> };
    close $out_0 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_0, 0;
    $result_0
}), '--', "$current");
    close $in_1 or croak 'Close failed: $OS_ERROR';
    my $result_1 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_1> };
    close $out_1 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_1, 0;
    $result_1
});
    return;
}

sub __docker_containers {
    my $format;
if ("${1-}" eq "--id") {
        $format = '{{.ID}}';
# Builtin command 'shift' not implemented
}
    else {
        if ("${1-}" eq "--name") {
            $format = '{{.Names}}';
# Builtin command 'shift' not implemented
}
        else {
            if ("${DOCKER_COMPLETION_SHOW_CONTAINER_IDS-}" eq yes) {
                $format = '{{.ID}} {{.Names}}';
}
            else {
                $format = '{{.Names}}';
            }
        }
    }
    __docker_q('ps', '--format', "$format", "\@ARGV");
    return;
}

sub __docker_complete_containers {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_3, $out_3);
    my $pid_3 = open3($in_3, $out_3, '>&STDERR', 'compgen', '-W', (do {
    my ($in_2, $out_2);
    my $pid_2 = open3($in_2, $out_2, '>&STDERR', '__docker_containers', "\@ARGV");
    close $in_2 or croak 'Close failed: $OS_ERROR';
    my $result_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_2> };
    close $out_2 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_2, 0;
    $result_2
}), '--', "$current");
    close $in_3 or croak 'Close failed: $OS_ERROR';
    my $result_3 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_3> };
    close $out_3 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_3, 0;
    $result_3
});
    return;
}

sub __docker_complete_containers_all {
    __docker_complete_containers("\@ARGV", '--all');
    return;
}

sub __docker_complete_containers_removable {
    __docker_complete_containers("\@ARGV", '--filter', 'status', q{=}, 'created', '--filter', 'status', q{=}, 'exited');
    return;
}

sub __docker_complete_containers_running {
    __docker_complete_containers("\@ARGV", '--filter', 'status', q{=}, 'running');
    return;
}

sub __docker_complete_containers_stoppable {
    __docker_complete_containers("\@ARGV", '--filter', 'status', q{=}, 'running', '--filter', 'status', q{=}, 'paused');
    return;
}

sub __docker_complete_containers_stopped {
    __docker_complete_containers("\@ARGV", '--filter', 'status', q{=}, 'exited');
    return;
}

sub __docker_complete_containers_unpauseable {
    __docker_complete_containers("\@ARGV", '--filter', 'status', q{=}, 'paused');
    return;
}

sub __docker_complete_container_names {
    my @containers = ('$(__docker_q ps -aq --no-trunc');
    my @names = ('$(__docker_q inspect --format \'{{.Name}}\' "${containers[@]}"');
    @names = (@names);
    my @COMPREPLY = (do {
    my ($in_4, $out_4);
    my $pid_4 = open3($in_4, $out_4, '>&STDERR', 'compgen', '-W', $names[eval { int(*) } // ""], '--', "$ENV{cur}");
    close $in_4 or croak 'Close failed: $OS_ERROR';
    my $result_4 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_4> };
    close $out_4 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_4, 0;
    $result_4
});
    return;
}

sub __docker_complete_container_ids {
    my @containers = ('$(__docker_q ps -aq');
    my @COMPREPLY = (do {
    my ($in_5, $out_5);
    my $pid_5 = open3($in_5, $out_5, '>&STDERR', 'compgen', '-W', $containers[eval { int(*) } // ""], '--', "$ENV{cur}");
    close $in_5 or croak 'Close failed: $OS_ERROR';
    my $result_5 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_5> };
    close $out_5 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_5, 0;
    $result_5
});
    return;
}

sub __docker_contexts {
    my @add = ();
while ( 1 ) {
if ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--add') {
                        push @add, '$2';
            # Builtin command 'shift' not implemented
        } elsif (1) {
            last;        }
    }
    __docker_q('context', 'ls', '-q');
    say @add;
    return;
}

sub __docker_complete_contexts {
    my @contexts = ('$(__docker_contexts "$@"');
    my @COMPREPLY = (do {
    my ($in_7, $out_7);
    my $pid_7 = open3($in_7, $out_7, '>&STDERR', 'compgen', '-W', $contexts[eval { int(*) } // ""], '--', "$ENV{cur}");
    close $in_7 or croak 'Close failed: $OS_ERROR';
    my $result_7 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_7> };
    close $out_7 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_7, 0;
    $result_7
});
    return;
}

sub __docker_images {
    my $repo_format = "{{.Repository}}";
    my $tag_format = "{{.Repository}}:{{.Tag}}";
    my $id_format = "{{.ID}}";
    my $all;
    my $format;
if ("${DOCKER_COMPLETION_SHOW_IMAGE_IDS-}" eq "all") {
        $all = '--all';
    }
while ( 1 ) {
if ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--repo') {
                        $format = "$repo_format\n";
            # Builtin command 'shift' not implemented
        } elsif ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--tag') {
            if ("${DOCKER_COMPLETION_SHOW_TAGS:-yes}" eq "yes") {
                $format = "$tag_format\n";
            }
            # Builtin command 'shift' not implemented
        } elsif ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--id') {
            if ($ENV{DOCKER_COMPLETION_SHOW_IMAGE_IDS-} =~ /^(all|non-intermediate)$/msx) {
                $format = "$id_format\n";
            }
            # Builtin command 'shift' not implemented
        } elsif ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--force-tag') {
                        $format = "$tag_format\n";
            # Builtin command 'shift' not implemented
        } elsif (1) {
            last;        }
    }
    # Original bash: __docker_q image ls --no-trunc --format "${format%\\n}" ${all-} "$@" | grep -v '<none>$'
do {
        my $output_9 = q{};
        my $output_printed_9;
        my $pipeline_success_9 = 1;
                my ($in_10, $out_10);
        my $pid_10 = open3($in_10, $out_10, '>&STDERR', '__docker_q', 'image', 'ls', '--no-trunc', '--format');
        close $in_10 or croak 'Close failed: $OS_ERROR';
        $output_9 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_10> };
        close $out_10 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_10, 0;

                my $grep_result_9_1;
        my @grep_lines_9_1 = split /\n/msx, $output_9;
        my @grep_filtered_9_1 = grep { !/<none>$/msx } @grep_lines_9_1;
        $grep_result_9_1 = join "\n", @grep_filtered_9_1;
        if (!($grep_result_9_1 =~ m{\n\z} || $grep_result_9_1 eq q{})) {
        $grep_result_9_1 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_9_1 > 0 ? 0 : 1;
        $output_9 = $grep_result_9_1;
        $output_9 = $grep_result_9_1;
        if ((scalar @grep_filtered_9_1) == 0) {
            $pipeline_success_9 = 0;
        }
        if ($output_9 ne q{} && !defined $output_printed_9) {
            print $output_9;
            if (!($output_9 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_9 ) { $main_exit_code = 1; }
        }
;
    return;
}

sub __docker_complete_images {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_12, $out_12);
    my $pid_12 = open3($in_12, $out_12, '>&STDERR', 'compgen', '-W', (do {
    my ($in_11, $out_11);
    my $pid_11 = open3($in_11, $out_11, '>&STDERR', '__docker_images', "\@ARGV");
    close $in_11 or croak 'Close failed: $OS_ERROR';
    my $result_11 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_11> };
    close $out_11 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_11, 0;
    $result_11
}), '--', "$current");
    close $in_12 or croak 'Close failed: $OS_ERROR';
    my $result_12 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_12> };
    close $out_12 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_12, 0;
    $result_12
});
    $main_exit_code = system('__ltrim_colon_completions', "$current") >> 8;
    return;
}

sub __docker_networks {
    my $format;
if ("${1-}" eq "--id") {
        $format = '{{.ID}}';
# Builtin command 'shift' not implemented
}
    else {
        if ("${1-}" eq "--name") {
            $format = '{{.Name}}';
# Builtin command 'shift' not implemented
}
        else {
            if ("${DOCKER_COMPLETION_SHOW_NETWORK_IDS-}" eq yes) {
                $format = '{{.ID}} {{.Name}}';
}
            else {
                $format = '{{.Name}}';
            }
        }
    }
    __docker_q('network', 'ls', '--format', "$format", "\@ARGV");
    return;
}

sub __docker_complete_networks {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_14, $out_14);
    my $pid_14 = open3($in_14, $out_14, '>&STDERR', 'compgen', '-W', (do {
    my ($in_13, $out_13);
    my $pid_13 = open3($in_13, $out_13, '>&STDERR', '__docker_networks', "\@ARGV");
    close $in_13 or croak 'Close failed: $OS_ERROR';
    my $result_13 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_13> };
    close $out_13 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_13, 0;
    $result_13
}), '--', "$current");
    close $in_14 or croak 'Close failed: $OS_ERROR';
    my $result_14 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_14> };
    close $out_14 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_14, 0;
    $result_14
});
    return;
}

sub __docker_complete_containers_in_network {
    my @containers = ('$(__docker_q network inspect -f \'{{range $i, $c := .Containers}}{{$i}} {{$c.Name}} {{end}}\' "$1")');
    my @COMPREPLY = (do {
    my ($in_15, $out_15);
    my $pid_15 = open3($in_15, $out_15, '>&STDERR', 'compgen', '-W', $containers[eval { int(*) } // ""], '--', "$ENV{cur}");
    close $in_15 or croak 'Close failed: $OS_ERROR';
    my $result_15 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_15> };
    close $out_15 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_15, 0;
    $result_15
});
    return;
}

sub __docker_volumes {
    __docker_q('volume', 'ls', '-q', "\@ARGV");
    return;
}

sub __docker_complete_volumes {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_17, $out_17);
    my $pid_17 = open3($in_17, $out_17, '>&STDERR', 'compgen', '-W', (do {
    my ($in_16, $out_16);
    my $pid_16 = open3($in_16, $out_16, '>&STDERR', '__docker_volumes', "\@ARGV");
    close $in_16 or croak 'Close failed: $OS_ERROR';
    my $result_16 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_16> };
    close $out_16 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_16, 0;
    $result_16
}), '--', "$current");
    close $in_17 or croak 'Close failed: $OS_ERROR';
    my $result_17 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_17> };
    close $out_17 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_17, 0;
    $result_17
});
    return;
}

sub __docker_plugins_bundled {
    my ($file) = @_;
    my $type;
    my @add = ();
    my @remove = ();
while ( 1 ) {
if ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--type') {
                        $type = "$_[1]";
            # Builtin command 'shift' not implemented
        } elsif ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--add') {
                        push @add, '$_[1]';
            # Builtin command 'shift' not implemented
        } elsif ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--remove') {
                        push @remove, '$_[1]';
            # Builtin command 'shift' not implemented
        } elsif (1) {
            last;        }
    }
    my @plugins = ('$(__docker_q info --format "{{range \$i, \$p := .Plugins.$type}}{{.}} {{end}}")');
    my $del;
    for my $del (@remove) {
        @plugins = (@plugins);
    }
;
    say @plugins . q{ } . @add;
    return;
}

sub __docker_complete_plugins_bundled {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_20, $out_20);
    my $pid_20 = open3($in_20, $out_20, '>&STDERR', 'compgen', '-W', (do {
    my ($in_19, $out_19);
    my $pid_19 = open3($in_19, $out_19, '>&STDERR', '__docker_plugins_bundled', "\@ARGV");
    close $in_19 or croak 'Close failed: $OS_ERROR';
    my $result_19 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_19> };
    close $out_19 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_19, 0;
    $result_19
}), '--', "$current");
    close $in_20 or croak 'Close failed: $OS_ERROR';
    my $result_20 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_20> };
    close $out_20 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_20, 0;
    $result_20
});
    return;
}

sub __docker_plugins_installed {
    my $format;
if ("${DOCKER_COMPLETION_SHOW_PLUGIN_IDS-}" eq yes) {
        $format = '{{.ID}} {{.Name}}';
}
    else {
        $format = '{{.Name}}';
    }
    __docker_q('plugin', 'ls', '--format', "$format", "\@ARGV");
    return;
}

sub __docker_complete_plugins_installed {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_22, $out_22);
    my $pid_22 = open3($in_22, $out_22, '>&STDERR', 'compgen', '-W', (do {
    my ($in_21, $out_21);
    my $pid_21 = open3($in_21, $out_21, '>&STDERR', '__docker_plugins_installed', "\@ARGV");
    close $in_21 or croak 'Close failed: $OS_ERROR';
    my $result_21 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_21> };
    close $out_21 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_21, 0;
    $result_21
}), '--', "$current");
    close $in_22 or croak 'Close failed: $OS_ERROR';
    my $result_22 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_22> };
    close $out_22 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_22, 0;
    $result_22
});
    return;
}

sub __docker_runtimes {
    # Original bash: __docker_q info | sed -n 's/^Runtimes: \(.*\)/\1/p'
do {
        my $output_23 = q{};
        my $output_printed_23;
        my $pipeline_success_23 = 1;
                my ($in_24, $out_24);
        my $pid_24 = open3($in_24, $out_24, '>&STDERR', '__docker_q', 'info');
        close $in_24 or croak 'Close failed: $OS_ERROR';
        $output_23 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_24> };
        close $out_24 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_24, 0;

                my @sed_lines_23 = split /\n/, $output_23;
        my @sed_result_23;
        foreach my $line (@sed_lines_23) {
        chomp $line;
        push @sed_result_23, $line;
        }
        $output_23 = join "\n", @sed_result_23;
        if ($output_23 ne q{} && !defined $output_printed_23) {
            print $output_23;
            if (!($output_23 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_23 ) { $main_exit_code = 1; }
        }
;
    return;
}

sub __docker_complete_runtimes {
    my @COMPREPLY = (do {
    my ($in_26, $out_26);
    my $pid_26 = open3($in_26, $out_26, '>&STDERR', 'compgen', '-W', (do {
    my ($in_25, $out_25);
    my $pid_25 = open3($in_25, $out_25, '>&STDERR', '__docker_runtimes');
    close $in_25 or croak 'Close failed: $OS_ERROR';
    my $result_25 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_25> };
    close $out_25 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_25, 0;
    $result_25
}), '--', "$ENV{cur}");
    close $in_26 or croak 'Close failed: $OS_ERROR';
    my $result_26 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_26> };
    close $out_26 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_26, 0;
    $result_26
});
    return;
}

sub __docker_secrets {
    my $format;
if ("${1-}" eq "--id") {
        $format = '{{.ID}}';
# Builtin command 'shift' not implemented
}
    else {
        if ("${1-}" eq "--name") {
            $format = '{{.Name}}';
# Builtin command 'shift' not implemented
}
        else {
            if ("${DOCKER_COMPLETION_SHOW_SECRET_IDS-}" eq yes) {
                $format = '{{.ID}} {{.Name}}';
}
            else {
                $format = '{{.Name}}';
            }
        }
    }
    __docker_q('secret', 'ls', '--format', "$format", "\@ARGV");
    return;
}

sub __docker_complete_secrets {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_28, $out_28);
    my $pid_28 = open3($in_28, $out_28, '>&STDERR', 'compgen', '-W', (do {
    my ($in_27, $out_27);
    my $pid_27 = open3($in_27, $out_27, '>&STDERR', '__docker_secrets', "\@ARGV");
    close $in_27 or croak 'Close failed: $OS_ERROR';
    my $result_27 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_27> };
    close $out_27 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_27, 0;
    $result_27
}), '--', "$current");
    close $in_28 or croak 'Close failed: $OS_ERROR';
    my $result_28 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_28> };
    close $out_28 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_28, 0;
    $result_28
});
    return;
}

sub __docker_stacks {
    my ($file) = @_;
    # Original bash: __docker_q stack ls | awk 'NR>1 {print $_[0]}'
do {
        my $output_29 = q{};
        my $output_printed_29;
        my $pipeline_success_29 = 1;
                my ($in_30, $out_30);
        my $pid_30 = open3($in_30, $out_30, '>&STDERR', '__docker_q', 'stack', 'ls');
        close $in_30 or croak 'Close failed: $OS_ERROR';
        $output_29 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_30> };
        close $out_30 or croak 'Close failed: $OS_ERROR';
        waitpid $pid_30, 0;

                my @lines = split /\n/, $output_29;
        my @result;
        my $NR = 0;
        foreach my $line (@lines) {
        chomp $line;
        if ($line =~ /^\s*$/msx) { next; }
        $NR++;
        my @fields = split /\s+/msx, $line;
        if (!($NR>1)) { next; }
        push @result, ($fields[0] . "\n");
        }
        $output_29 = join "", @result;
        if ($output_29 ne q{} && !defined $output_printed_29) {
            print $output_29;
            if (!($output_29 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_29 ) { $main_exit_code = 1; }
        }
;
    return;
}

sub __docker_complete_stacks {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_32, $out_32);
    my $pid_32 = open3($in_32, $out_32, '>&STDERR', 'compgen', '-W', (do {
    my ($in_31, $out_31);
    my $pid_31 = open3($in_31, $out_31, '>&STDERR', '__docker_stacks', "\@ARGV");
    close $in_31 or croak 'Close failed: $OS_ERROR';
    my $result_31 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_31> };
    close $out_31 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_31, 0;
    $result_31
}), '--', "$current");
    close $in_32 or croak 'Close failed: $OS_ERROR';
    my $result_32 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_32> };
    close $out_32 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_32, 0;
    $result_32
});
    return;
}

sub __docker_nodes {
    my $format;
if ("${DOCKER_COMPLETION_SHOW_NODE_IDS-}" eq yes) {
        $format = '{{.ID}} {{.Hostname}}';
}
    else {
        $format = '{{.Hostname}}';
    }
    my @add = ();
while ( 1 ) {
if ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--id') {
                        $format = '{{.ID}}';
            # Builtin command 'shift' not implemented
        } elsif ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--name') {
                        $format = '{{.Hostname}}';
            # Builtin command 'shift' not implemented
        } elsif ((defined (defined $_[0] && $_[0] ne q{} ? $_[0] : '') && (defined $_[0] && $_[0] ne q{} ? $_[0] : '') ne q{} ? (defined $_[0] && $_[0] ne q{} ? $_[0] : '') : '') eq '--add') {
                        push @add, '$2';
            # Builtin command 'shift' not implemented
        } elsif (1) {
            last;        }
    }
    say (do {
    my ($in_34, $out_34);
    my $pid_34 = open3($in_34, $out_34, '>&STDERR', '__docker_q', 'node', 'ls', '--format', "$format", "\@ARGV");
    close $in_34 or croak 'Close failed: $OS_ERROR';
    my $result_34 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_34> };
    close $out_34 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_34, 0;
    $result_34
}) . q{ } . @add;
    return;
}

sub __docker_complete_nodes {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_36, $out_36);
    my $pid_36 = open3($in_36, $out_36, '>&STDERR', 'compgen', '-W', (do {
    my ($in_35, $out_35);
    my $pid_35 = open3($in_35, $out_35, '>&STDERR', '__docker_nodes', "\@ARGV");
    close $in_35 or croak 'Close failed: $OS_ERROR';
    my $result_35 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_35> };
    close $out_35 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_35, 0;
    $result_35
}), '--', "$current");
    close $in_36 or croak 'Close failed: $OS_ERROR';
    my $result_36 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_36> };
    close $out_36 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_36, 0;
    $result_36
});
    return;
}

sub __docker_services {
    my $format = "{{.Name}}";
    if ("${DOCKER_COMPLETION_SHOW_SERVICE_IDS-}" eq yes) {
                $format = '{{.ID}} {{.Name}}';
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
if ("${1-}" eq "--id") {
        $format = '{{.ID}}';
# Builtin command 'shift' not implemented
}
    else {
        if ("${1-}" eq "--name") {
            $format = '{{.Name}}';
# Builtin command 'shift' not implemented
        }
    }
    __docker_q('service', 'ls', '--quiet', '--format', "$format", "\@ARGV");
    return;
}

sub __docker_complete_services {
    my ($file) = @_;
    my $current = "$ENV{cur}";
if ("${1-}" eq "--cur") {
        $current = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_37, $out_37);
    my $pid_37 = open3($in_37, $out_37, '>&STDERR', '__docker_services', "\@ARGV", '--filter', "name=$current");
    close $in_37 or croak 'Close failed: $OS_ERROR';
    my $result_37 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_37> };
    close $out_37 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_37, 0;
    $result_37
});
    return;
}

sub __docker_tasks {
    __docker_q('service', 'ps', '--format', '{{.ID}}', "");
    return;
}

sub __docker_complete_services_and_tasks {
    my @COMPREPLY = (do {
    my ($in_40, $out_40);
    my $pid_40 = open3($in_40, $out_40, '>&STDERR', 'compgen', '-W', (do {
    my ($in_38, $out_38);
    my $pid_38 = open3($in_38, $out_38, '>&STDERR', '__docker_services', "\@ARGV");
    close $in_38 or croak 'Close failed: $OS_ERROR';
    my $result_38 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_38> };
    close $out_38 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_38, 0;
    $result_38
}) . " " . (do {
    my ($in_39, $out_39);
    my $pid_39 = open3($in_39, $out_39, '>&STDERR', '__docker_tasks');
    close $in_39 or croak 'Close failed: $OS_ERROR';
    my $result_39 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_39> };
    close $out_39 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_39, 0;
    $result_39
}), '--', "$ENV{cur}");
    close $in_40 or croak 'Close failed: $OS_ERROR';
    my $result_40 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_40> };
    close $out_40 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_40, 0;
    $result_40
});
    return;
}

sub __docker_append_to_completions {
    my @COMPREPLY = (@COMPREPLY);
    return;
}

sub __docker_fetch_info {
    my $info_fetched;
if ("${info_fetched-}" eq q{}) {
$server_experimental = <>;
chomp $server_experimental;
$CHILD_ERROR = defined($server_experimental) ? 0 : 1;
        $info_fetched = 'true';
    }
;
    return;
}

sub __docker_server_is_experimental {
    __docker_fetch_info();
"$server_experimental" eq "true"
    return;
}

sub __docker_server_os_is {
    my ($file) = @_;
    my $expected_os = "$_[0]";
    __docker_fetch_info();
"$server_os" eq "$expected_os"
    return;
}

sub __docker_pos_first_nonflag {
    my $argument_flags = $1-;
    my $counter = eval { int((defined $ENV{subcommand_pos} && $ENV{subcommand_pos} ne q{} ? $ENV{subcommand_pos} : $ENV{command_pos}) + 1) } // "";
while ( $counter <= $cword ) {
if (("$argument_flags" ne q{} && !(do { my $eval_input = "case '" . $words[eval { int($counter) } // ""] . "' in " . $argument_flags . ") true ;; *) false ;; esac"; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; }))) {
            $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
            if (0) {
                                $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
                $CHILD_ERROR = 0;
            } else {
                $CHILD_ERROR = 1;
            }
}
        else {
if ($words[eval { int($counter) } // ""] =~ /^-.*$/msx) {
            } elsif (1) {
                last;            }
        }
while ( 0 ) {
            $counter = eval { int( $counter + 2) } // "";
        }
        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
    }
    say $counter;
    return;
}

sub __docker_map_key_of_current_option {
    my ($file) = @_;
    my $glob = "$_[0]";
    my $key;
    my $glob_pos;
if (0) {
        $key = "$ENV{prev}";
        my $cword;
        $glob_pos = eval { int($cword - 2) } // "";
}
    else {
        if ($cur =~ /[*]=[*]/msx) {
            $key = scalar reverse( (scalar reverse ($ENV{cur} // q{})) =~ s/^.*?=//r );
            $glob_pos = eval { int($cword - 1) } // "";
}
        else {
            if (0) {
                $key = q{};
                $glob_pos = eval { int($cword - 3) } // "";
}
            else {
return;
            }
        }
    }
    if (0) {
                $CHILD_ERROR = ($main_exit_code = eval { int($glob_pos--) } // "") ? 0 : 1;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    if (${words[$glob_pos]} =~ /[(][?]:[$]glob[)]/msx) {
                say $key;
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    return;
}

sub __docker_value_of_option {
    my ($file) = @_;
    my $option_extglob = do {
    my ($in_43, $out_43);
    my $pid_43 = open3($in_43, $out_43, '>&STDERR', '__docker_to_extglob', "$_[0]");
    close $in_43 or croak 'Close failed: $OS_ERROR';
    my $result_43 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_43> };
    close $out_43 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_43, 0;
    $result_43
};
    my $counter = eval { int($ENV{command_pos} + 1) } // "";
while ( $counter < $cword ) {
if (q{} eq '$option_extglob') {
                        say $words[eval { int($counter + 1) } // ""];
            last;        }
        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
    }
    return;
}

sub __docker_to_alternatives {
    my @parts = ('$1');
    my $IFS = "|";
    say $parts[eval { int(*) } // ""];
    return;
}

sub __docker_to_extglob {
    my ($file) = @_;
    my $extglob = do {
    my ($in_44, $out_44);
    my $pid_44 = open3($in_44, $out_44, '>&STDERR', '__docker_to_alternatives', "$_[0]");
    close $in_44 or croak 'Close failed: $OS_ERROR';
    my $result_44 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_44> };
    close $out_44 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_44, 0;
    $result_44
};
    say "\@($extglob)";
    return;
}

sub __docker_subcommands {
    my ($file) = @_;
    my $subcommands = "$_[0]";
    my $counter = eval { int($ENV{command_pos} + 1) } // "";
    my $subcommand_pos;
while ( $counter < $cword ) {
if ($words[eval { int($counter) } // ""] eq '$(__docker_to_extglob "$subcommands")') {
                        $subcommand_pos = $counter;
                        my $subcommand = "words[$counter]";
                        my $completions_func = $command;
            my $_;
                        if (do {
                                do {
                    open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
                    open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
                    open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
                    close $original_stdout
      or die "Close failed: $OS_ERROR\n";
                };
            } == 0) {
                                $CHILD_ERROR = 0;
            }
            return q{0};        }
        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
    }
;
return q{1};
    return;
}

sub __docker_nospace {
    if (do {
                do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
            $main_exit_code = system('type', 'compopt') >> 8;
            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    } == 0) {
                $main_exit_code = system('compopt', '-o', 'nospace') >> 8;
    }
    return;
}

sub __docker_complete_resolved_hostname {
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', 'host') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
        return;
    }
;
    my @COMPREPLY = ('$(host 2>/dev/null "${cur%:}" | awk \'/has address/ {print $4}\'');
    return;
}

sub __docker_local_interfaces {
        do {
        open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
        open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
local *STDERR;
open STDERR, '>&', STDOUT or die "Cannot dup stderr: $OS_ERROR\n";
        my $tmp = do {
        $main_exit_code = system('command', '-v', 'ip') >> 8;
        };
        print $tmp;
        open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
        close $original_stdout
      or die "Close failed: $OS_ERROR\n";
    };
    if ($CHILD_ERROR != 0) {
        return;
    }
;
    my $format;
if ("${1-}" eq "--ip-only") {
        $format = "\\1";
# Builtin command 'shift' not implemented
}
    else {
        $format = "\\1 \\2";
    }
    # Original bash: ip addr show scope global 2>/dev/null | sed -n "s| \+inet \([0-9.]\+\).* \([^ ]\+\)|$format|p"
do {
        my $output_46 = q{};
        my $output_printed_46;
        my $pipeline_success_46 = 1;
                $output = q{};
                do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
my $tmp_redirect_47 = q{};

my $cmd_50 = 'ip';
my ($in_49, $out_49);
my $pid_49 = open3($in_49, $out_49, '>&STDERR', $cmd_50, 'addr', 'show', 'scope', 'global');
print {$in_49} $output_46;
close $in_49 or croak 'Close failed: $OS_ERROR';
$tmp_redirect_47 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_49> };
close $out_49 or croak 'Close failed: $OS_ERROR';
waitpid $pid_49, 0;
$tmp_redirect_47;
        };
        $output_46 = $output;

                my @sed_lines_46 = split /\n/, $output_46;
        my @sed_result_46;
        foreach my $line (@sed_lines_46) {
        chomp $line;
        push @sed_result_46, $line;
        }
        $output_46 = join "\n", @sed_result_46;
        if ($output_46 ne q{} && !defined $output_printed_46) {
            print $output_46;
            if (!($output_46 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_46 ) { $main_exit_code = 1; }
        }
;
    return;
}

sub __docker_complete_local_interfaces {
    my ($file) = @_;
    my $additional_interface;
if ("${1-}" eq "--add") {
        $additional_interface = "$_[1]";
# Builtin command 'shift' not implemented
    }
    my @COMPREPLY = (do {
    my ($in_52, $out_52);
    my $pid_52 = open3($in_52, $out_52, '>&STDERR', 'compgen', '-W', (do {
    my ($in_51, $out_51);
    my $pid_51 = open3($in_51, $out_51, '>&STDERR', '__docker_local_interfaces', "\@ARGV");
    close $in_51 or croak 'Close failed: $OS_ERROR';
    my $result_51 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_51> };
    close $out_51 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_51, 0;
    $result_51
}) . " $additional_interface", '--', "$ENV{cur}");
    close $in_52 or croak 'Close failed: $OS_ERROR';
    my $result_52 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_52> };
    close $out_52 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_52, 0;
    $result_52
});
    return;
}

sub __docker_complete_local_ips {
    __docker_complete_local_interfaces('--ip-only');
    return;
}

sub __docker_complete_capabilities_addable {
    my @capabilities = ('ALL', 'CAP_AUDIT_CONTROL', 'CAP_AUDIT_READ', 'CAP_BLOCK_SUSPEND', 'CAP_BPF', 'CAP_CHECKPOINT_RESTORE', 'CAP_DAC_READ_SEARCH', 'CAP_IPC_LOCK', 'CAP_IPC_OWNER', 'CAP_LEASE', 'CAP_LINUX_IMMUTABLE', 'CAP_MAC_ADMIN', 'CAP_MAC_OVERRIDE', 'CAP_NET_ADMIN', 'CAP_NET_BROADCAST', 'CAP_PERFMON', 'CAP_SYS_ADMIN', 'CAP_SYS_BOOT', 'CAP_SYSLOG', 'CAP_SYS_MODULE', 'CAP_SYS_NICE', 'CAP_SYS_PACCT', 'CAP_SYS_PTRACE', 'CAP_SYS_RAWIO', 'CAP_SYS_RESOURCE', 'CAP_SYS_TIME', 'CAP_SYS_TTY_CONFIG', 'CAP_WAKE_ALARM', 'RESET');
    my @COMPREPLY = (do {
    my ($in_53, $out_53);
    my $pid_53 = open3($in_53, $out_53, '>&STDERR', 'compgen', '-W', $capabilities[eval { int(*) } // ""] . " " . ($capabilities[eval { int(*) } // ""] =~ s/^CAP_//r), '--', "$ENV{cur}");
    close $in_53 or croak 'Close failed: $OS_ERROR';
    my $result_53 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_53> };
    close $out_53 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_53, 0;
    $result_53
});
    return;
}

sub __docker_complete_capabilities_droppable {
    my @capabilities = ('ALL', 'CAP_AUDIT_WRITE', 'CAP_CHOWN', 'CAP_DAC_OVERRIDE', 'CAP_FOWNER', 'CAP_FSETID', 'CAP_KILL', 'CAP_MKNOD', 'CAP_NET_BIND_SERVICE', 'CAP_NET_RAW', 'CAP_SETFCAP', 'CAP_SETGID', 'CAP_SETPCAP', 'CAP_SETUID', 'CAP_SYS_CHROOT', 'RESET');
    my @COMPREPLY = (do {
    my ($in_54, $out_54);
    my $pid_54 = open3($in_54, $out_54, '>&STDERR', 'compgen', '-W', $capabilities[eval { int(*) } // ""] . " " . ($capabilities[eval { int(*) } // ""] =~ s/^CAP_//r), '--', "$ENV{cur}");
    close $in_54 or croak 'Close failed: $OS_ERROR';
    my $result_54 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_54> };
    close $out_54 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_54, 0;
    $result_54
});
    return;
}

sub __docker_complete_detach_keys {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--detach-keys') {
        if ("$ENV{cur}" =~ /^.*,$/msx) {
                        @COMPREPLY = (do {
    my ($in_55, $out_55);
    my $pid_55 = open3($in_55, $out_55, '>&STDERR', 'compgen', '-W', ($ENV{cur} // q{}) . "ctrl-", '--', "$ENV{cur}");
    close $in_55 or croak 'Close failed: $OS_ERROR';
    my $result_55 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_55> };
    close $out_55 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_55, 0;
    $result_55
});
        } elsif (1) {
                        @COMPREPLY = (do {
    my ($in_56, $out_56);
    my $pid_56 = open3($in_56, $out_56, '>&STDERR', 'compgen', '-W', "ctrl-", '--', "$ENV{cur}");
    close $in_56 or croak 'Close failed: $OS_ERROR';
    my $result_56 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_56> };
    close $out_56 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_56, 0;
    $result_56
});
        }
                __docker_nospace();
        return;    }
;
return q{1};
    return;
}

sub __docker_complete_isolation {
    my @COMPREPLY = (do {
    my ($in_57, $out_57);
    my $pid_57 = open3($in_57, $out_57, '>&STDERR', 'compgen', '-W', "default hyperv process", '--', "$ENV{cur}");
    close $in_57 or croak 'Close failed: $OS_ERROR';
    my $result_57 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_57> };
    close $out_57 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_57, 0;
    $result_57
});
    return;
}

sub __docker_complete_log_drivers {
    my @COMPREPLY = (do {
    my ($in_58, $out_58);
    my $pid_58 = open3($in_58, $out_58, '>&STDERR', 'compgen', '-W', "
		awslogs
		etwlogs
		fluentd
		gcplogs
		gelf
		journald
		json-file
		local
		none
		splunk
		syslog
	", '--', "$ENV{cur}");
    close $in_58 or croak 'Close failed: $OS_ERROR';
    my $result_58 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_58> };
    close $out_58 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_58, 0;
    $result_58
});
    return;
}

sub __docker_complete_log_options {
    my $common_options1 = "max-buffer-size mode";
    my $common_options2 = "env env-regex labels";
    my $awslogs_options = "$common_options1 awslogs-create-group awslogs-credentials-endpoint awslogs-datetime-format awslogs-group awslogs-multiline-pattern awslogs-region awslogs-stream tag";
    my $fluentd_options = "$common_options1 $common_options2 fluentd-address fluentd-async fluentd-buffer-limit fluentd-request-ack fluentd-retry-wait fluentd-max-retries fluentd-sub-second-precision tag";
    my $gcplogs_options = "$common_options1 $common_options2 gcp-log-cmd gcp-meta-id gcp-meta-name gcp-meta-zone gcp-project";
    my $gelf_options = "$common_options1 $common_options2 gelf-address gelf-compression-level gelf-compression-type gelf-tcp-max-reconnect gelf-tcp-reconnect-delay tag";
    my $journald_options = "$common_options1 $common_options2 tag";
    my $json_file_options = "$common_options1 $common_options2 compress max-file max-size";
    my $local_options = "$common_options1 compress max-file max-size";
    my $splunk_options = "$common_options1 $common_options2 splunk-caname splunk-capath splunk-format splunk-gzip splunk-gzip-level splunk-index splunk-insecureskipverify splunk-source splunk-sourcetype splunk-token splunk-url splunk-verify-connection tag";
    my $syslog_options = "$common_options1 $common_options2 syslog-address syslog-facility syslog-format syslog-tls-ca-cert syslog-tls-cert syslog-tls-key syslog-tls-skip-verify tag";
    my $all_options = "$fluentd_options $gcplogs_options $gelf_options $journald_options $json_file_options $syslog_options $splunk_options";
    my $COMPREPLY;
if (do {
    my ($in_59, $out_59);
    my $pid_59 = open3($in_59, $out_59, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_59 or croak 'Close failed: $OS_ERROR';
    my $result_59 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_59> };
    close $out_59 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_59, 0;
    $result_59
} eq '') {
                @COMPREPLY = (do {
    my ($in_60, $out_60);
    my $pid_60 = open3($in_60, $out_60, '>&STDERR', 'compgen', '-W', "$all_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_60 or croak 'Close failed: $OS_ERROR';
    my $result_60 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_60> };
    close $out_60 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_60, 0;
    $result_60
});
    } elsif (do {
    my ($in_61, $out_61);
    my $pid_61 = open3($in_61, $out_61, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_61 or croak 'Close failed: $OS_ERROR';
    my $result_61 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_61> };
    close $out_61 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_61, 0;
    $result_61
} eq 'awslogs') {
                @COMPREPLY = (do {
    my ($in_62, $out_62);
    my $pid_62 = open3($in_62, $out_62, '>&STDERR', 'compgen', '-W', "$awslogs_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_62 or croak 'Close failed: $OS_ERROR';
    my $result_62 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_62> };
    close $out_62 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_62, 0;
    $result_62
});
    } elsif (do {
    my ($in_63, $out_63);
    my $pid_63 = open3($in_63, $out_63, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_63 or croak 'Close failed: $OS_ERROR';
    my $result_63 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_63> };
    close $out_63 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_63, 0;
    $result_63
} eq 'fluentd') {
                @COMPREPLY = (do {
    my ($in_64, $out_64);
    my $pid_64 = open3($in_64, $out_64, '>&STDERR', 'compgen', '-W', "$fluentd_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_64 or croak 'Close failed: $OS_ERROR';
    my $result_64 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_64> };
    close $out_64 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_64, 0;
    $result_64
});
    } elsif (do {
    my ($in_65, $out_65);
    my $pid_65 = open3($in_65, $out_65, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_65 or croak 'Close failed: $OS_ERROR';
    my $result_65 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_65> };
    close $out_65 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_65, 0;
    $result_65
} eq 'gcplogs') {
                @COMPREPLY = (do {
    my ($in_66, $out_66);
    my $pid_66 = open3($in_66, $out_66, '>&STDERR', 'compgen', '-W', "$gcplogs_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_66 or croak 'Close failed: $OS_ERROR';
    my $result_66 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_66> };
    close $out_66 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_66, 0;
    $result_66
});
    } elsif (do {
    my ($in_67, $out_67);
    my $pid_67 = open3($in_67, $out_67, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_67 or croak 'Close failed: $OS_ERROR';
    my $result_67 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_67> };
    close $out_67 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_67, 0;
    $result_67
} eq 'gelf') {
                @COMPREPLY = (do {
    my ($in_68, $out_68);
    my $pid_68 = open3($in_68, $out_68, '>&STDERR', 'compgen', '-W', "$gelf_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_68 or croak 'Close failed: $OS_ERROR';
    my $result_68 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_68> };
    close $out_68 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_68, 0;
    $result_68
});
    } elsif (do {
    my ($in_69, $out_69);
    my $pid_69 = open3($in_69, $out_69, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_69 or croak 'Close failed: $OS_ERROR';
    my $result_69 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_69> };
    close $out_69 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_69, 0;
    $result_69
} eq 'journald') {
                @COMPREPLY = (do {
    my ($in_70, $out_70);
    my $pid_70 = open3($in_70, $out_70, '>&STDERR', 'compgen', '-W', "$journald_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_70 or croak 'Close failed: $OS_ERROR';
    my $result_70 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_70> };
    close $out_70 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_70, 0;
    $result_70
});
    } elsif (do {
    my ($in_71, $out_71);
    my $pid_71 = open3($in_71, $out_71, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_71 or croak 'Close failed: $OS_ERROR';
    my $result_71 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_71> };
    close $out_71 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_71, 0;
    $result_71
} eq 'json-file') {
                @COMPREPLY = (do {
    my ($in_72, $out_72);
    my $pid_72 = open3($in_72, $out_72, '>&STDERR', 'compgen', '-W', "$json_file_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_72 or croak 'Close failed: $OS_ERROR';
    my $result_72 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_72> };
    close $out_72 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_72, 0;
    $result_72
});
    } elsif (do {
    my ($in_73, $out_73);
    my $pid_73 = open3($in_73, $out_73, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_73 or croak 'Close failed: $OS_ERROR';
    my $result_73 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_73> };
    close $out_73 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_73, 0;
    $result_73
} eq 'local') {
                @COMPREPLY = (do {
    my ($in_74, $out_74);
    my $pid_74 = open3($in_74, $out_74, '>&STDERR', 'compgen', '-W', "$local_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_74 or croak 'Close failed: $OS_ERROR';
    my $result_74 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_74> };
    close $out_74 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_74, 0;
    $result_74
});
    } elsif (do {
    my ($in_75, $out_75);
    my $pid_75 = open3($in_75, $out_75, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_75 or croak 'Close failed: $OS_ERROR';
    my $result_75 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_75> };
    close $out_75 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_75, 0;
    $result_75
} eq 'syslog') {
                @COMPREPLY = (do {
    my ($in_76, $out_76);
    my $pid_76 = open3($in_76, $out_76, '>&STDERR', 'compgen', '-W', "$syslog_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_76 or croak 'Close failed: $OS_ERROR';
    my $result_76 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_76> };
    close $out_76 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_76, 0;
    $result_76
});
    } elsif (do {
    my ($in_77, $out_77);
    my $pid_77 = open3($in_77, $out_77, '>&STDERR', '__docker_value_of_option', '--log-driver');
    close $in_77 or croak 'Close failed: $OS_ERROR';
    my $result_77 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_77> };
    close $out_77 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_77, 0;
    $result_77
} eq 'splunk') {
                @COMPREPLY = (do {
    my ($in_78, $out_78);
    my $pid_78 = open3($in_78, $out_78, '>&STDERR', 'compgen', '-W', "$splunk_options", '-S', q{=}, '--', "$ENV{cur}");
    close $in_78 or croak 'Close failed: $OS_ERROR';
    my $result_78 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_78> };
    close $out_78 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_78, 0;
    $result_78
});
    } elsif (1) {
        return;    }
;
    __docker_nospace();
    return;
}

sub __docker_complete_log_driver_options {
    my $key = do {
    my ($in_79, $out_79);
    my $pid_79 = open3($in_79, $out_79, '>&STDERR', '__docker_map_key_of_current_option', '--log-opt');
    close $in_79 or croak 'Close failed: $OS_ERROR';
    my $result_79 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_79> };
    close $out_79 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_79, 0;
    $result_79
};
    my $COMPREPLY;
if ("$key" eq 'awslogs-create-group') {
                @COMPREPLY = (do {
    my ($in_80, $out_80);
    my $pid_80 = open3($in_80, $out_80, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_80 or croak 'Close failed: $OS_ERROR';
    my $result_80 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_80> };
    close $out_80 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_80, 0;
    $result_80
});
        return;    } elsif ("$key" eq 'awslogs-credentials-endpoint') {
                @COMPREPLY = (do {
    my ($in_81, $out_81);
    my $pid_81 = open3($in_81, $out_81, '>&STDERR', 'compgen', '-W', "/", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_81 or croak 'Close failed: $OS_ERROR';
    my $result_81 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_81> };
    close $out_81 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_81, 0;
    $result_81
});
                __docker_nospace();
        return;    } elsif ("$key" eq 'compress' or "$key" eq 'fluentd-async-connect') {
                @COMPREPLY = (do {
    my ($in_82, $out_82);
    my $pid_82 = open3($in_82, $out_82, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_82 or croak 'Close failed: $OS_ERROR';
    my $result_82 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_82> };
    close $out_82 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_82, 0;
    $result_82
});
        return;    } elsif ("$key" eq 'fluentd-sub-second-precision') {
                @COMPREPLY = (do {
    my ($in_83, $out_83);
    my $pid_83 = open3($in_83, $out_83, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_83 or croak 'Close failed: $OS_ERROR';
    my $result_83 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_83> };
    close $out_83 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_83, 0;
    $result_83
});
        return;    } elsif ("$key" eq 'gelf-address') {
                @COMPREPLY = (do {
    my ($in_84, $out_84);
    my $pid_84 = open3($in_84, $out_84, '>&STDERR', 'compgen', '-W', "tcp udp", '-S', "://", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_84 or croak 'Close failed: $OS_ERROR';
    my $result_84 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_84> };
    close $out_84 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_84, 0;
    $result_84
});
                __docker_nospace();
        return;    } elsif ("$key" eq 'gelf-compression-level') {
                @COMPREPLY = (do {
    my ($in_85, $out_85);
    my $pid_85 = open3($in_85, $out_85, '>&STDERR', 'compgen', '-W', "1 2 3 4 5 6 7 8 9", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_85 or croak 'Close failed: $OS_ERROR';
    my $result_85 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_85> };
    close $out_85 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_85, 0;
    $result_85
});
        return;    } elsif ("$key" eq 'gelf-compression-type') {
                @COMPREPLY = (do {
    my ($in_86, $out_86);
    my $pid_86 = open3($in_86, $out_86, '>&STDERR', 'compgen', '-W', "gzip none zlib", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_86 or croak 'Close failed: $OS_ERROR';
    my $result_86 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_86> };
    close $out_86 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_86, 0;
    $result_86
});
        return;    } elsif ("$key" eq 'line-only') {
                @COMPREPLY = (do {
    my ($in_87, $out_87);
    my $pid_87 = open3($in_87, $out_87, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_87 or croak 'Close failed: $OS_ERROR';
    my $result_87 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_87> };
    close $out_87 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_87, 0;
    $result_87
});
        return;    } elsif ("$key" eq 'mode') {
                @COMPREPLY = (do {
    my ($in_88, $out_88);
    my $pid_88 = open3($in_88, $out_88, '>&STDERR', 'compgen', '-W', "blocking non-blocking", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_88 or croak 'Close failed: $OS_ERROR';
    my $result_88 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_88> };
    close $out_88 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_88, 0;
    $result_88
});
        return;    } elsif ("$key" eq 'syslog-address') {
                @COMPREPLY = (do {
    my ($in_89, $out_89);
    my $pid_89 = open3($in_89, $out_89, '>&STDERR', 'compgen', '-W', "tcp:// tcp+tls:// udp:// unix://", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_89 or croak 'Close failed: $OS_ERROR';
    my $result_89 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_89> };
    close $out_89 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_89, 0;
    $result_89
});
                __docker_nospace();
                $main_exit_code = system('__ltrim_colon_completions', ($ENV{cur} // q{})) >> 8;
        return;    } elsif ("$key" eq 'syslog-facility') {
                @COMPREPLY = (do {
    my ($in_90, $out_90);
    my $pid_90 = open3($in_90, $out_90, '>&STDERR', 'compgen', '-W', "
				auth
				authpriv
				cron
				daemon
				ftp
				kern
				local0
				local1
				local2
				local3
				local4
				local5
				local6
				local7
				lpr
				mail
				news
				syslog
				user
				uucp
			", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_90 or croak 'Close failed: $OS_ERROR';
    my $result_90 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_90> };
    close $out_90 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_90, 0;
    $result_90
});
        return;    } elsif ("$key" eq 'syslog-format') {
                @COMPREPLY = (do {
    my ($in_91, $out_91);
    my $pid_91 = open3($in_91, $out_91, '>&STDERR', 'compgen', '-W', "rfc3164 rfc5424 rfc5424micro", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_91 or croak 'Close failed: $OS_ERROR';
    my $result_91 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_91> };
    close $out_91 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_91, 0;
    $result_91
});
        return;    } elsif ("$key" eq 'syslog-tls-ca-cert' or "$key" eq 'syslog-tls-cert' or "$key" eq 'syslog-tls-key') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    } elsif ("$key" eq 'syslog-tls-skip-verify') {
                @COMPREPLY = (do {
    my ($in_92, $out_92);
    my $pid_92 = open3($in_92, $out_92, '>&STDERR', 'compgen', '-W', "true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_92 or croak 'Close failed: $OS_ERROR';
    my $result_92 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_92> };
    close $out_92 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_92, 0;
    $result_92
});
        return;    } elsif ("$key" eq 'splunk-url') {
                @COMPREPLY = (do {
    my ($in_93, $out_93);
    my $pid_93 = open3($in_93, $out_93, '>&STDERR', 'compgen', '-W', "http:// https://", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_93 or croak 'Close failed: $OS_ERROR';
    my $result_93 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_93> };
    close $out_93 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_93, 0;
    $result_93
});
                __docker_nospace();
                $main_exit_code = system('__ltrim_colon_completions', ($ENV{cur} // q{})) >> 8;
        return;    } elsif ("$key" eq 'splunk-gzip' or "$key" eq 'splunk-insecureskipverify' or "$key" eq 'splunk-verify-connection') {
                @COMPREPLY = (do {
    my ($in_94, $out_94);
    my $pid_94 = open3($in_94, $out_94, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_94 or croak 'Close failed: $OS_ERROR';
    my $result_94 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_94> };
    close $out_94 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_94, 0;
    $result_94
});
        return;    } elsif ("$key" eq 'splunk-format') {
                @COMPREPLY = (do {
    my ($in_95, $out_95);
    my $pid_95 = open3($in_95, $out_95, '>&STDERR', 'compgen', '-W', "inline json raw", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_95 or croak 'Close failed: $OS_ERROR';
    my $result_95 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_95> };
    close $out_95 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_95, 0;
    $result_95
});
        return;    }
;
return q{1};
    return;
}

sub __docker_complete_log_levels {
    my @COMPREPLY = (do {
    my ($in_96, $out_96);
    my $pid_96 = open3($in_96, $out_96, '>&STDERR', 'compgen', '-W', "debug info warn error fatal", '--', "$ENV{cur}");
    close $in_96 or croak 'Close failed: $OS_ERROR';
    my $result_96 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_96> };
    close $out_96 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_96, 0;
    $result_96
});
    return;
}

sub __docker_complete_restart {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--restart') {
        if ("$ENV{cur}" =~ /^on-failure:.*$/msx) {
        } elsif (1) {
                        @COMPREPLY = (do {
    my ($in_97, $out_97);
    my $pid_97 = open3($in_97, $out_97, '>&STDERR', 'compgen', '-W', "always no on-failure on-failure: unless-stopped", '--', "$ENV{cur}");
    close $in_97 or croak 'Close failed: $OS_ERROR';
    my $result_97 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_97> };
    close $out_97 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_97, 0;
    $result_97
});
        }
        return;    }
;
return q{1};
    return;
}

sub __docker_complete_signals {
    my @signals = ('SIGCONT', 'SIGHUP', 'SIGINT', 'SIGKILL', 'SIGQUIT', 'SIGSTOP', 'SIGTERM', 'SIGUSR1', 'SIGUSR2');
    my @COMPREPLY = ('$( compgen -W "${signals[*]} ${signals[*]#SIG}" -- "$( echo "$cur" | tr \'[:lower:]\' \'[:upper:]\')"');
    return;
}

sub __docker_complete_ulimits {
    my $limits = "
		as
		chroot
		core
		cpu
		data
		fsize
		locks
		maxlogins
		maxsyslogins
		memlock
		msgqueue
		nice
		nofile
		nproc
		priority
		rss
		rtprio
		sigpending
		stack
	";
    my $COMPREPLY;
if ("${1-}" eq "--rm") {
        @COMPREPLY = (do {
    my ($in_100, $out_100);
    my $pid_100 = open3($in_100, $out_100, '>&STDERR', 'compgen', '-W', "$limits", '--', "$ENV{cur}");
    close $in_100 or croak 'Close failed: $OS_ERROR';
    my $result_100 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_100> };
    close $out_100 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_100, 0;
    $result_100
});
}
    else {
        @COMPREPLY = (do {
    my ($in_101, $out_101);
    my $pid_101 = open3($in_101, $out_101, '>&STDERR', 'compgen', '-W', "$limits", '-S', q{=}, '--', "$ENV{cur}");
    close $in_101 or croak 'Close failed: $OS_ERROR';
    my $result_101 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_101> };
    close $out_101 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_101, 0;
    $result_101
});
        __docker_nospace();
    }
;
    return;
}

sub __docker_complete_user_group {
    my $COMPREPLY;
if ($cur =~ /[*]:[*]/msx) {
        @COMPREPLY = (do {
    my ($in_102, $out_102);
    my $pid_102 = open3($in_102, $out_102, '>&STDERR', 'compgen', '-g', '--', (($ENV{cur} // q{}) =~ s/^.*?://r =~ s/^.*?://r));
    close $in_102 or croak 'Close failed: $OS_ERROR';
    my $result_102 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_102> };
    close $out_102 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_102, 0;
    $result_102
});
}
    else {
        @COMPREPLY = (do {
    my ($in_103, $out_103);
    my $pid_103 = open3($in_103, $out_103, '>&STDERR', 'compgen', '-u', '-S', q{:}, '--', "$ENV{cur}");
    close $in_103 or croak 'Close failed: $OS_ERROR';
    my $result_103 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_103> };
    close $out_103 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_103, 0;
    $result_103
});
        __docker_nospace();
    }
;
    return;
}

sub __docker_plugins_path {
    my $docker_plugins_path = do {
    my ($in_104, $out_104);
    my $pid_104 = open3($in_104, $out_104, '>&STDERR', 'docker', 'info', '--format', '{{range .ClientInfo.Plugins}}{{.Path}}:{{end}}');
    close $in_104 or croak 'Close failed: $OS_ERROR';
    my $result_104 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_104> };
    close $out_104 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_104, 0;
    $result_104
};
    say $docker_plugins_path =~ s/:/ /grs;
    return;
}

sub __docker_complete_plugin {
    my ($file) = @_;
    my $path = $_[0];
    my $completionCommand = "__completeNoDesc";
    my @resultArray = ('$path', '$completionCommand');
    my $current = "$ENV{cur}";
    my $value;
    for my $value (join(" ", @words[2..$#words])) {
if ("$value" eq q{}) {
            push @resultArray, '\'\'';
}
        else {
            push @resultArray, $value;
        }
    }
;
    my $rawResult = do { my @_qx_cmd = (": 'Complex command not supported in bash string generation' 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; };
    my $result = do { my $input_data = ""; my $grep_result_105;
my @grep_lines_105 = split /\n/msx, $input_data;
my @grep_filtered_105 = grep { !/^:[0-9]*$/msx } @grep_lines_105;
$grep_result_105 = join "\n", @grep_filtered_105;
    if (!($grep_result_105 =~ m{\n\z} || $grep_result_105 eq q{})) {
        $grep_result_105 .= "\n";
    }
$CHILD_ERROR = scalar @grep_filtered_105 > 0 ? 0 : 1;
 };
    my $completionFlag = do { my $here_input = "$rawResult"; chomp(my $result = qx{echo "$here_input" | tail -1}); $CHILD_ERROR = $? >> 8; $result; };
if ("$completionFlag" =~ /":8"/msx) {
        my $filePattern = do { my $input_data = ""; my $set1_107 = "\\n";
my $set2_107 = q{|};
my $input_107 = $input_data;
# Expand character ranges for tr command
my $expanded_set1_107 = $set1_107;
my $expanded_set2_107 = $set2_107;
# Handle a-z range in set1
if ($expanded_set1_107 =~ /a-z/msx) {
    $expanded_set1_107 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set1
if ($expanded_set1_107 =~ /A-Z/msx) {
    $expanded_set1_107 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set1
if ($expanded_set1_107 =~ /\[:upper:\]/msx) {
    $expanded_set1_107 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set1
if ($expanded_set1_107 =~ /\[:lower:\]/msx) {
    $expanded_set1_107 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle a-z range in set2
if ($expanded_set2_107 =~ /a-z/msx) {
    $expanded_set2_107 =~ s/a-z/abcdefghijklmnopqrstuvwxyz/msx;
}
# Handle A-Z range in set2
if ($expanded_set2_107 =~ /A-Z/msx) {
    $expanded_set2_107 =~ s/A-Z/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:upper:] POSIX class in set2
if ($expanded_set2_107 =~ /\[:upper:\]/msx) {
    $expanded_set2_107 =~ s/\[:upper:\]/ABCDEFGHIJKLMNOPQRSTUVWXYZ/msx;
}
# Handle [:lower:] POSIX class in set2
if ($expanded_set2_107 =~ /\[:lower:\]/msx) {
    $expanded_set2_107 =~ s/\[:lower:\]/abcdefghijklmnopqrstuvwxyz/msx;
}
my $tr_result_106 = q{};
for my $char ( split //msx, $input_107 ) {
    my $pos_107 = index $expanded_set1_107, $char;
    if ( $pos_107 >= 0 && $pos_107 < length $expanded_set2_107 ) {
        $tr_result_106 .= substr $expanded_set2_107, $pos_107, 1;
    } else {
        $tr_result_106 .= $char;
    }
}
$tr_result_106 };
        $main_exit_code = system('_filedir', "$filePattern") >> 8;
return;
    }
    my $COMPREPLY;
if ("$result" eq q{}) {
        $main_exit_code = system('bash', '_filedir') >> 8;
}
    else {
        @COMPREPLY = (do {
    my ($in_108, $out_108);
    my $pid_108 = open3($in_108, $out_108, '>&STDERR', 'compgen', '-W', ${result}, '--', (defined (defined ${current} && ${current} ne q{} ? ${current} : '') && (defined ${current} && ${current} ne q{} ? ${current} : '') ne q{} ? (defined ${current} && ${current} ne q{} ? ${current} : '') : ''));
    close $in_108 or croak 'Close failed: $OS_ERROR';
    my $result_108 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_108> };
    close $out_108 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_108, 0;
    $result_108
});
    }
;
    return;
}

sub _docker_docker {
    my $boolean_options = "
		$ENV{global_boolean_options}
		--help
		--version -v
	";
if ("$ENV{prev}" eq '--config') {
                $main_exit_code = system('_filedir', '-d') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--context' or "$ENV{prev}" eq '-c') {
                __docker_complete_contexts();
        return;    } elsif ("$ENV{prev}" eq '--log-level' or "$ENV{prev}" eq '-l') {
                __docker_complete_log_levels();
        return;    } elsif ("$ENV{prev}" eq '$(__docker_to_extglob "$global_options_with_args")') {
        return;    }
    my $COMPREPLY;
    my $commands;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_109, $out_109);
    my $pid_109 = open3($in_109, $out_109, '>&STDERR', 'compgen', '-W', "$boolean_options $ENV{global_options_with_args}", '--', "$ENV{cur}");
    close $in_109 or croak 'Close failed: $OS_ERROR';
    my $result_109 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_109> };
    close $out_109 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_109, 0;
    $result_109
});
    } elsif (1) {
                my $counter = do {
    my ($in_111, $out_111);
    my $pid_111 = open3($in_111, $out_111, '>&STDERR', '__docker_pos_first_nonflag', (do {
    my ($in_110, $out_110);
    my $pid_110 = open3($in_110, $out_110, '>&STDERR', '__docker_to_extglob', "$ENV{global_options_with_args}");
    close $in_110 or croak 'Close failed: $OS_ERROR';
    my $result_110 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_110> };
    close $out_110 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_110, 0;
    $result_110
}));
    close $in_111 or croak 'Close failed: $OS_ERROR';
    my $result_111 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_111> };
    close $out_111 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_111, 0;
    $result_111
};
        if (($cword == $counter)) {
            if (do {
__docker_server_is_experimental();
                $CHILD_ERROR == 0
            }) {
                                @commands = ($experimental_server_commands[eval { int(*) } // ""]);
            }
            @COMPREPLY = (do {
    my ($in_112, $out_112);
    my $pid_112 = open3($in_112, $out_112, '>&STDERR', 'compgen', '-W', $commands[eval { int(*) } // ""] . " help", '--', "$ENV{cur}");
    close $in_112 or croak 'Close failed: $OS_ERROR';
    my $result_112 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_112> };
    close $out_112 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_112, 0;
    $result_112
});
        }
    }
;
    return;
}

sub _docker_attach {
    $main_exit_code = system('bash', '_docker_container_attach') >> 8;
    return;
}

sub _docker_build {
    $main_exit_code = system('bash', '_docker_image_build') >> 8;
    return;
}

sub _docker_builder {
    my $subcommands = "
		build
		prune
	";
    if (do {
__docker_subcommands("$subcommands");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_113, $out_113);
    my $pid_113 = open3($in_113, $out_113, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_113 or croak 'Close failed: $OS_ERROR';
    my $result_113 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_113> };
    close $out_113 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_113, 0;
    $result_113
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_114, $out_114);
    my $pid_114 = open3($in_114, $out_114, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_114 or croak 'Close failed: $OS_ERROR';
    my $result_114 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_114> };
    close $out_114 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_114, 0;
    $result_114
});
    }
;
    return;
}

sub _docker_builder_build {
    $main_exit_code = system('bash', '_docker_image_build') >> 8;
    return;
}

sub _docker_builder_prune {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter') {
                @COMPREPLY = (do {
    my ($in_115, $out_115);
    my $pid_115 = open3($in_115, $out_115, '>&STDERR', 'compgen', '-S', q{=}, '-W', "description id inuse parent private shared type until unused-for", '--', "$ENV{cur}");
    close $in_115 or croak 'Close failed: $OS_ERROR';
    my $result_115 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_115> };
    close $out_115 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_115, 0;
    $result_115
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--keep-storage') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_116, $out_116);
    my $pid_116 = open3($in_116, $out_116, '>&STDERR', 'compgen', '-W', "--all -a --filter --force -f --help --keep-storage", '--', "$ENV{cur}");
    close $in_116 or croak 'Close failed: $OS_ERROR';
    my $result_116 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_116> };
    close $out_116 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_116, 0;
    $result_116
});
    }
    return;
}

sub _docker_checkpoint {
    my $subcommands = "
		create
		ls
		rm
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_117, $out_117);
    my $pid_117 = open3($in_117, $out_117, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_117 or croak 'Close failed: $OS_ERROR';
    my $result_117 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_117> };
    close $out_117 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_117, 0;
    $result_117
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_118, $out_118);
    my $pid_118 = open3($in_118, $out_118, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_118 or croak 'Close failed: $OS_ERROR';
    my $result_118 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_118> };
    close $out_118 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_118, 0;
    $result_118
});
    }
;
    return;
}

sub _docker_checkpoint_create {
if ("$ENV{prev}" eq '--checkpoint-dir') {
                $main_exit_code = system('_filedir', '-d') >> 8;
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_119, $out_119);
    my $pid_119 = open3($in_119, $out_119, '>&STDERR', 'compgen', '-W', "--checkpoint-dir --help --leave-running", '--', "$ENV{cur}");
    close $in_119 or croak 'Close failed: $OS_ERROR';
    my $result_119 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_119> };
    close $out_119 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_119, 0;
    $result_119
});
    } elsif (1) {
                my $counter = do {
    my ($in_120, $out_120);
    my $pid_120 = open3($in_120, $out_120, '>&STDERR', '__docker_pos_first_nonflag', '--checkpoint-dir');
    close $in_120 or croak 'Close failed: $OS_ERROR';
    my $result_120 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_120> };
    close $out_120 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_120, 0;
    $result_120
};
        if (($cword == $counter)) {
            __docker_complete_containers_running();
        }
    }
;
    return;
}

sub _docker_checkpoint_ls {
if ("$ENV{prev}" eq '--checkpoint-dir') {
                $main_exit_code = system('_filedir', '-d') >> 8;
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_121, $out_121);
    my $pid_121 = open3($in_121, $out_121, '>&STDERR', 'compgen', '-W', "--checkpoint-dir --help", '--', "$ENV{cur}");
    close $in_121 or croak 'Close failed: $OS_ERROR';
    my $result_121 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_121> };
    close $out_121 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_121, 0;
    $result_121
});
    } elsif (1) {
                my $counter = do {
    my ($in_122, $out_122);
    my $pid_122 = open3($in_122, $out_122, '>&STDERR', '__docker_pos_first_nonflag', '--checkpoint-dir');
    close $in_122 or croak 'Close failed: $OS_ERROR';
    my $result_122 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_122> };
    close $out_122 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_122, 0;
    $result_122
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
        }
    }
;
    return;
}

sub _docker_checkpoint_rm {
if ("$ENV{prev}" eq '--checkpoint-dir') {
                $main_exit_code = system('_filedir', '-d') >> 8;
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_123, $out_123);
    my $pid_123 = open3($in_123, $out_123, '>&STDERR', 'compgen', '-W', "--checkpoint-dir --help", '--', "$ENV{cur}");
    close $in_123 or croak 'Close failed: $OS_ERROR';
    my $result_123 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_123> };
    close $out_123 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_123, 0;
    $result_123
});
    } elsif (1) {
                my $counter = do {
    my ($in_124, $out_124);
    my $pid_124 = open3($in_124, $out_124, '>&STDERR', '__docker_pos_first_nonflag', '--checkpoint-dir');
    close $in_124 or croak 'Close failed: $OS_ERROR';
    my $result_124 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_124> };
    close $out_124 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_124, 0;
    $result_124
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                @COMPREPLY = ('$( compgen -W "$(__docker_q checkpoint ls "$prev" | sed 1d)" -- "$cur"');
            }
        }
    }
;
    return;
}

sub _docker_config {
    my $subcommands = "
		create
		inspect
		ls
		rm
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_127, $out_127);
    my $pid_127 = open3($in_127, $out_127, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_127 or croak 'Close failed: $OS_ERROR';
    my $result_127 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_127> };
    close $out_127 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_127, 0;
    $result_127
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_128, $out_128);
    my $pid_128 = open3($in_128, $out_128, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_128 or croak 'Close failed: $OS_ERROR';
    my $result_128 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_128> };
    close $out_128 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_128, 0;
    $result_128
});
    }
;
    return;
}

sub _docker_config_create {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--label' or "$ENV{prev}" eq '-l') {
        return;    } elsif ("$ENV{prev}" eq '--template-driver') {
                @COMPREPLY = (do {
    my ($in_129, $out_129);
    my $pid_129 = open3($in_129, $out_129, '>&STDERR', 'compgen', '-W', "golang", '--', "$ENV{cur}");
    close $in_129 or croak 'Close failed: $OS_ERROR';
    my $result_129 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_129> };
    close $out_129 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_129, 0;
    $result_129
});
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_130, $out_130);
    my $pid_130 = open3($in_130, $out_130, '>&STDERR', 'compgen', '-W', "--help --label -l --template-driver", '--', "$ENV{cur}");
    close $in_130 or croak 'Close failed: $OS_ERROR';
    my $result_130 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_130> };
    close $out_130 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_130, 0;
    $result_130
});
    } elsif (1) {
                my $counter = do {
    my ($in_131, $out_131);
    my $pid_131 = open3($in_131, $out_131, '>&STDERR', '__docker_pos_first_nonflag', '--label|-l|--template-driver');
    close $in_131 or croak 'Close failed: $OS_ERROR';
    my $result_131 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_131> };
    close $out_131 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_131, 0;
    $result_131
};
        if (($cword == (eval { int($counter + 1) } // ""))) {
            $main_exit_code = system('bash', '_filedir') >> 8;
        }
    }
    return;
}

sub _docker_config_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_132, $out_132);
    my $pid_132 = open3($in_132, $out_132, '>&STDERR', 'compgen', '-W', "--format -f --help --pretty", '--', "$ENV{cur}");
    close $in_132 or croak 'Close failed: $OS_ERROR';
    my $result_132 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_132> };
    close $out_132 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_132, 0;
    $result_132
});
    } elsif (1) {
                __docker_complete_configs();
    }
;
    return;
}

sub _docker_config_list {
    $main_exit_code = system('bash', '_docker_config_ls') >> 8;
    return;
}

sub _docker_config_ls {
    my $key = do {
    my ($in_133, $out_133);
    my $pid_133 = open3($in_133, $out_133, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_133 or croak 'Close failed: $OS_ERROR';
    my $result_133 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_133> };
    close $out_133 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_133, 0;
    $result_133
};
if ("$key" eq 'id') {
                __docker_complete_configs('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_configs('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    }
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_134, $out_134);
    my $pid_134 = open3($in_134, $out_134, '>&STDERR', 'compgen', '-S', q{=}, '-W', "id label name", '--', "$ENV{cur}");
    close $in_134 or croak 'Close failed: $OS_ERROR';
    my $result_134 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_134> };
    close $out_134 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_134, 0;
    $result_134
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_135, $out_135);
    my $pid_135 = open3($in_135, $out_135, '>&STDERR', 'compgen', '-W', "--format --filter -f --help --quiet -q", '--', "$ENV{cur}");
    close $in_135 or croak 'Close failed: $OS_ERROR';
    my $result_135 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_135> };
    close $out_135 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_135, 0;
    $result_135
});
    }
    return;
}

sub _docker_config_remove {
    $main_exit_code = system('bash', '_docker_config_rm') >> 8;
    return;
}

sub _docker_config_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_136, $out_136);
    my $pid_136 = open3($in_136, $out_136, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_136 or croak 'Close failed: $OS_ERROR';
    my $result_136 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_136> };
    close $out_136 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_136, 0;
    $result_136
});
    } elsif (1) {
                __docker_complete_configs();
    }
;
    return;
}

sub _docker_container {
    my $subcommands = "
		attach
		commit
		cp
		create
		diff
		exec
		export
		inspect
		kill
		logs
		ls
		pause
		port
		prune
		rename
		restart
		rm
		run
		start
		stats
		stop
		top
		unpause
		update
		wait
	";
    my $aliases = "
		list
		ps
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_137, $out_137);
    my $pid_137 = open3($in_137, $out_137, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_137 or croak 'Close failed: $OS_ERROR';
    my $result_137 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_137> };
    close $out_137 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_137, 0;
    $result_137
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_138, $out_138);
    my $pid_138 = open3($in_138, $out_138, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_138 or croak 'Close failed: $OS_ERROR';
    my $result_138 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_138> };
    close $out_138 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_138, 0;
    $result_138
});
    }
;
    return;
}

sub _docker_container_attach {
    if (do {
__docker_complete_detach_keys();
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_139, $out_139);
    my $pid_139 = open3($in_139, $out_139, '>&STDERR', 'compgen', '-W', "--detach-keys --help --no-stdin --sig-proxy=false", '--', "$ENV{cur}");
    close $in_139 or croak 'Close failed: $OS_ERROR';
    my $result_139 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_139> };
    close $out_139 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_139, 0;
    $result_139
});
    } elsif (1) {
                my $counter = do {
    my ($in_140, $out_140);
    my $pid_140 = open3($in_140, $out_140, '>&STDERR', '__docker_pos_first_nonflag', '--detach-keys');
    close $in_140 or croak 'Close failed: $OS_ERROR';
    my $result_140 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_140> };
    close $out_140 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_140, 0;
    $result_140
};
        if (($cword == $counter)) {
            __docker_complete_containers_running();
        }
    }
;
    return;
}

sub _docker_container_commit {
if ("$ENV{prev}" eq '--author' or "$ENV{prev}" eq '-a' or "$ENV{prev}" eq '--change' or "$ENV{prev}" eq '-c' or "$ENV{prev}" eq '--message' or "$ENV{prev}" eq '-m') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_141, $out_141);
    my $pid_141 = open3($in_141, $out_141, '>&STDERR', 'compgen', '-W', "--author -a --change -c --help --message -m --pause=false -p=false", '--', "$ENV{cur}");
    close $in_141 or croak 'Close failed: $OS_ERROR';
    my $result_141 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_141> };
    close $out_141 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_141, 0;
    $result_141
});
    } elsif (1) {
                my $counter = do {
    my ($in_142, $out_142);
    my $pid_142 = open3($in_142, $out_142, '>&STDERR', '__docker_pos_first_nonflag', '--author|-a|--change|-c|--message|-m');
    close $in_142 or croak 'Close failed: $OS_ERROR';
    my $result_142 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_142> };
    close $out_142 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_142, 0;
    $result_142
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
return;
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                __docker_complete_images('--repo', '--tag');
return;
            }
        }
    }
;
    return;
}

sub _docker_container_cp {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_143, $out_143);
    my $pid_143 = open3($in_143, $out_143, '>&STDERR', 'compgen', '-W', "--archive -a --follow-link -L --help", '--', "$ENV{cur}");
    close $in_143 or croak 'Close failed: $OS_ERROR';
    my $result_143 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_143> };
    close $out_143 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_143, 0;
    $result_143
});
    } elsif (1) {
                my $counter = do {
    my ($in_144, $out_144);
    my $pid_144 = open3($in_144, $out_144, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_144 or croak 'Close failed: $OS_ERROR';
    my $result_144 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_144> };
    close $out_144 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_144, 0;
    $result_144
};
        if (($cword == $counter)) {
if ("$ENV{cur}" =~ /^.*:$/msx) {
                return;            } elsif (1) {
                                $main_exit_code = system('bash', '_filedir') >> 8;
                                my @files = ('${COMPREPLY[@]}');
                                __docker_complete_containers_all();
                                @COMPREPLY = (do {
    my ($in_145, $out_145);
    my $pid_145 = open3($in_145, $out_145, '>&STDERR', 'compgen', '-W', $COMPREPLY[eval { int(*) } // ""], '-S', q{:});
    close $in_145 or croak 'Close failed: $OS_ERROR';
    my $result_145 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_145> };
    close $out_145 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_145, 0;
    $result_145
});
                                my @containers = ('${COMPREPLY[@]}');
                                @COMPREPLY = (do {
    my ($in_146, $out_146);
    my $pid_146 = open3($in_146, $out_146, '>&STDERR', 'compgen', '-W', $files[eval { int(*) } // ""] . " " . $containers[eval { int(*) } // ""], '--', "$ENV{cur}");
    close $in_146 or croak 'Close failed: $OS_ERROR';
    my $result_146 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_146> };
    close $out_146 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_146, 0;
    $result_146
});
                if ("${COMPREPLY[*]}" eq *:) {
                    __docker_nospace();
                }
                return;            }
        }
                $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
        if (($cword == $counter)) {
if ((-e "$prev")) {
                __docker_complete_containers_all();
                @COMPREPLY = (do {
    my ($in_147, $out_147);
    my $pid_147 = open3($in_147, $out_147, '>&STDERR', 'compgen', '-W', $COMPREPLY[eval { int(*) } // ""], '-S', q{:});
    close $in_147 or croak 'Close failed: $OS_ERROR';
    my $result_147 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_147> };
    close $out_147 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_147, 0;
    $result_147
});
                __docker_nospace();
}
            else {
                $main_exit_code = system('bash', '_filedir') >> 8;
            }
return;
        }
    }
;
    return;
}

sub _docker_container_create {
    $main_exit_code = system('bash', '_docker_container_run_and_create') >> 8;
    return;
}

sub _docker_container_diff {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_148, $out_148);
    my $pid_148 = open3($in_148, $out_148, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_148 or croak 'Close failed: $OS_ERROR';
    my $result_148 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_148> };
    close $out_148 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_148, 0;
    $result_148
});
    } elsif (1) {
                my $counter = do {
    my ($in_149, $out_149);
    my $pid_149 = open3($in_149, $out_149, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_149 or croak 'Close failed: $OS_ERROR';
    my $result_149 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_149> };
    close $out_149 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_149, 0;
    $result_149
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
        }
    }
;
    return;
}

sub _docker_container_exec {
    if (do {
__docker_complete_detach_keys();
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{prev}" eq '--env' or "$ENV{prev}" eq '-e') {
                @COMPREPLY = (do {
    my ($in_150, $out_150);
    my $pid_150 = open3($in_150, $out_150, '>&STDERR', 'compgen', '-e', '--', "$ENV{cur}");
    close $in_150 or croak 'Close failed: $OS_ERROR';
    my $result_150 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_150> };
    close $out_150 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_150, 0;
    $result_150
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--env-file') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--user' or "$ENV{prev}" eq '-u') {
                __docker_complete_user_group();
        return;    } elsif ("$ENV{prev}" eq '--workdir' or "$ENV{prev}" eq '-w') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_151, $out_151);
    my $pid_151 = open3($in_151, $out_151, '>&STDERR', 'compgen', '-W', "--detach -d --detach-keys --env -e --env-file --help --interactive -i --privileged -t --tty -u --user --workdir -w", '--', "$ENV{cur}");
    close $in_151 or croak 'Close failed: $OS_ERROR';
    my $result_151 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_151> };
    close $out_151 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_151, 0;
    $result_151
});
    } elsif (1) {
                __docker_complete_containers_running();
    }
    return;
}

sub _docker_container_export {
if ("$ENV{prev}" eq '--output' or "$ENV{prev}" eq '-o') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_152, $out_152);
    my $pid_152 = open3($in_152, $out_152, '>&STDERR', 'compgen', '-W', "--help --output -o", '--', "$ENV{cur}");
    close $in_152 or croak 'Close failed: $OS_ERROR';
    my $result_152 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_152> };
    close $out_152 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_152, 0;
    $result_152
});
    } elsif (1) {
                my $counter = do {
    my ($in_153, $out_153);
    my $pid_153 = open3($in_153, $out_153, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_153 or croak 'Close failed: $OS_ERROR';
    my $result_153 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_153> };
    close $out_153 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_153, 0;
    $result_153
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
        }
    }
;
    return;
}

sub _docker_container_inspect {
    $main_exit_code = system('_docker_inspect', '--type', 'container') >> 8;
    return;
}

sub _docker_container_kill {
if ("$ENV{prev}" eq '--signal' or "$ENV{prev}" eq '-s') {
                __docker_complete_signals();
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_154, $out_154);
    my $pid_154 = open3($in_154, $out_154, '>&STDERR', 'compgen', '-W', "--help --signal -s", '--', "$ENV{cur}");
    close $in_154 or croak 'Close failed: $OS_ERROR';
    my $result_154 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_154> };
    close $out_154 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_154, 0;
    $result_154
});
    } elsif (1) {
                __docker_complete_containers_running();
    }
;
    return;
}

sub _docker_container_logs {
if ("$ENV{prev}" eq '--since' or "$ENV{prev}" eq '--tail' or "$ENV{prev}" eq '-n' or "$ENV{prev}" eq '--until') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_155, $out_155);
    my $pid_155 = open3($in_155, $out_155, '>&STDERR', 'compgen', '-W', "--details --follow -f --help --since --tail -n --timestamps -t --until", '--', "$ENV{cur}");
    close $in_155 or croak 'Close failed: $OS_ERROR';
    my $result_155 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_155> };
    close $out_155 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_155, 0;
    $result_155
});
    } elsif (1) {
                my $counter = do {
    my ($in_156, $out_156);
    my $pid_156 = open3($in_156, $out_156, '>&STDERR', '__docker_pos_first_nonflag', '--since|--tail|-n|--until');
    close $in_156 or croak 'Close failed: $OS_ERROR';
    my $result_156 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_156> };
    close $out_156 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_156, 0;
    $result_156
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
        }
    }
;
    return;
}

sub _docker_container_list {
    $main_exit_code = system('bash', '_docker_container_ls') >> 8;
    return;
}

sub _docker_container_ls {
    my $key = do {
    my ($in_157, $out_157);
    my $pid_157 = open3($in_157, $out_157, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_157 or croak 'Close failed: $OS_ERROR';
    my $result_157 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_157> };
    close $out_157 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_157, 0;
    $result_157
};
    my $COMPREPLY;
if ("$key" eq 'ancestor') {
                __docker_complete_images('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--repo', '--tag', '--id');
        return;    } elsif ("$key" eq 'before') {
                __docker_complete_containers_all('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    } elsif ("$key" eq 'expose' or "$key" eq 'publish') {
        return;    } elsif ("$key" eq 'id') {
                __docker_complete_containers_all('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'health') {
                @COMPREPLY = (do {
    my ($in_158, $out_158);
    my $pid_158 = open3($in_158, $out_158, '>&STDERR', 'compgen', '-W', "healthy starting none unhealthy", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_158 or croak 'Close failed: $OS_ERROR';
    my $result_158 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_158> };
    close $out_158 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_158, 0;
    $result_158
});
        return;    } elsif ("$key" eq 'is-task') {
                @COMPREPLY = (do {
    my ($in_159, $out_159);
    my $pid_159 = open3($in_159, $out_159, '>&STDERR', 'compgen', '-W', "true false", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_159 or croak 'Close failed: $OS_ERROR';
    my $result_159 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_159> };
    close $out_159 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_159, 0;
    $result_159
});
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_containers_all('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    } elsif ("$key" eq 'network') {
                __docker_complete_networks('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    } elsif ("$key" eq 'since') {
                __docker_complete_containers_all('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    } elsif ("$key" eq 'status') {
                @COMPREPLY = (do {
    my ($in_160, $out_160);
    my $pid_160 = open3($in_160, $out_160, '>&STDERR', 'compgen', '-W', "created dead exited paused restarting running removing", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_160 or croak 'Close failed: $OS_ERROR';
    my $result_160 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_160> };
    close $out_160 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_160, 0;
    $result_160
});
        return;    } elsif ("$key" eq 'volume') {
                __docker_complete_volumes('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_161, $out_161);
    my $pid_161 = open3($in_161, $out_161, '>&STDERR', 'compgen', '-S', q{=}, '-W', "ancestor before exited expose health id is-task label name network publish since status volume", '--', "$ENV{cur}");
    close $in_161 or croak 'Close failed: $OS_ERROR';
    my $result_161 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_161> };
    close $out_161 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_161, 0;
    $result_161
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '--last' or "$ENV{prev}" eq '-n') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_162, $out_162);
    my $pid_162 = open3($in_162, $out_162, '>&STDERR', 'compgen', '-W', "--all -a --filter -f --format --help --last -n --latest -l --no-trunc --quiet -q --size -s", '--', "$ENV{cur}");
    close $in_162 or croak 'Close failed: $OS_ERROR';
    my $result_162 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_162> };
    close $out_162 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_162, 0;
    $result_162
});
    }
    return;
}

sub _docker_container_pause {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_163, $out_163);
    my $pid_163 = open3($in_163, $out_163, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_163 or croak 'Close failed: $OS_ERROR';
    my $result_163 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_163> };
    close $out_163 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_163, 0;
    $result_163
});
    } elsif (1) {
                __docker_complete_containers_running();
    }
;
    return;
}

sub _docker_container_port {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_164, $out_164);
    my $pid_164 = open3($in_164, $out_164, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_164 or croak 'Close failed: $OS_ERROR';
    my $result_164 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_164> };
    close $out_164 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_164, 0;
    $result_164
});
    } elsif (1) {
                my $counter = do {
    my ($in_165, $out_165);
    my $pid_165 = open3($in_165, $out_165, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_165 or croak 'Close failed: $OS_ERROR';
    my $result_165 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_165> };
    close $out_165 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_165, 0;
    $result_165
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
        }
    }
;
    return;
}

sub _docker_container_prune {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter') {
                @COMPREPLY = (do {
    my ($in_166, $out_166);
    my $pid_166 = open3($in_166, $out_166, '>&STDERR', 'compgen', '-W', "label label! until", '-S', q{=}, '--', "$ENV{cur}");
    close $in_166 or croak 'Close failed: $OS_ERROR';
    my $result_166 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_166> };
    close $out_166 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_166, 0;
    $result_166
});
                __docker_nospace();
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_167, $out_167);
    my $pid_167 = open3($in_167, $out_167, '>&STDERR', 'compgen', '-W', "--force -f --filter --help", '--', "$ENV{cur}");
    close $in_167 or croak 'Close failed: $OS_ERROR';
    my $result_167 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_167> };
    close $out_167 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_167, 0;
    $result_167
});
    }
    return;
}

sub _docker_container_ps {
    _docker_container_ls();
    return;
}

sub _docker_container_rename {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_168, $out_168);
    my $pid_168 = open3($in_168, $out_168, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_168 or croak 'Close failed: $OS_ERROR';
    my $result_168 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_168> };
    close $out_168 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_168, 0;
    $result_168
});
    } elsif (1) {
                my $counter = do {
    my ($in_169, $out_169);
    my $pid_169 = open3($in_169, $out_169, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_169 or croak 'Close failed: $OS_ERROR';
    my $result_169 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_169> };
    close $out_169 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_169, 0;
    $result_169
};
        if (($cword == $counter)) {
            __docker_complete_containers_all();
        }
    }
;
    return;
}

sub _docker_container_restart {
if ("$ENV{prev}" eq '--time' or "$ENV{prev}" eq '-t') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_170, $out_170);
    my $pid_170 = open3($in_170, $out_170, '>&STDERR', 'compgen', '-W', "--help --time -t", '--', "$ENV{cur}");
    close $in_170 or croak 'Close failed: $OS_ERROR';
    my $result_170 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_170> };
    close $out_170 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_170, 0;
    $result_170
});
    } elsif (1) {
                __docker_complete_containers_all();
    }
;
    return;
}

sub _docker_container_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_171, $out_171);
    my $pid_171 = open3($in_171, $out_171, '>&STDERR', 'compgen', '-W', "--force -f --help --link -l --volumes -v", '--', "$ENV{cur}");
    close $in_171 or croak 'Close failed: $OS_ERROR';
    my $result_171 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_171> };
    close $out_171 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_171, 0;
    $result_171
});
    } elsif (1) {
                my $arg;
        for my $arg (@COMP_WORDS) {
if ("$arg" eq '--force' or "$arg" eq '-f') {
                                __docker_complete_containers_all();
                return;            }
        }
                __docker_complete_containers_removable();
    }
;
    return;
}

sub _docker_container_run {
    $main_exit_code = system('bash', '_docker_container_run_and_create') >> 8;
    return;
}

sub _docker_container_run_and_create {
    my $options_with_args = "
		--add-host
		--annotation
		--attach -a
		--blkio-weight
		--blkio-weight-device
		--cap-add
		--cap-drop
		--cgroupns
		--cgroup-parent
		--cidfile
		--cpu-period
		--cpu-quota
		--cpu-rt-period
		--cpu-rt-runtime
		--cpuset-cpus
		--cpus
		--cpuset-mems
		--cpu-shares -c
		--device
		--device-cgroup-rule
		--device-read-bps
		--device-read-iops
		--device-write-bps
		--device-write-iops
		--dns
		--dns-option
		--dns-search
		--domainname
		--entrypoint
		--env -e
		--env-file
		--expose
		--gpus
		--group-add
		--health-cmd
		--health-interval
		--health-retries
		--health-start-period
		--health-timeout
		--hostname -h
		--ip
		--ip6
		--ipc
		--kernel-memory
		--label-file
		--label -l
		--link
		--link-local-ip
		--log-driver
		--log-opt
		--mac-address
		--memory -m
		--memory-swap
		--memory-swappiness
		--memory-reservation
		--mount
		--name
		--network
		--network-alias
		--oom-score-adj
		--pid
		--pids-limit
		--platform
		--publish -p
		--pull
		--restart
		--runtime
		--security-opt
		--shm-size
		--stop-signal
		--stop-timeout
		--storage-opt
		--tmpfs
		--sysctl
		--ulimit
		--user -u
		--userns
		--uts
		--volume-driver
		--volumes-from
		--volume -v
		--workdir -w
	";
    if (do {
__docker_server_os_is('windows');
        $CHILD_ERROR == 0
    }) {
                my $cpu;
        my $count;
        my $percent;
        my $io;
        my $maxbandwidth;
        my $maxiops;
        my $isolation;
        $options_with_args = eval { int($options_with_args+"
		--$cpu-$count
		--$cpu-$percent
		--$io-$maxbandwidth
		--$io-$maxiops
		--$isolation
	") } // "";
    }
    my $boolean_options = "
		--disable-content-trust=false
		--help
		--init
		--interactive -i
		--no-healthcheck
		--oom-kill-disable
		--privileged
		--publish-all -P
		--quiet -q
		--read-only
		--tty -t
	";
if (("$command" eq "run" || "$subcommand" eq "run")) {
        $options_with_args = "$options_with_args
			--detach-keys
		";
        $boolean_options = "$boolean_options
			--detach -d
			--rm
			--sig-proxy=false
		";
        if (do {
__docker_complete_detach_keys();
            $CHILD_ERROR == 0
        }) {
            return;
        }
    }
    my $all_options = "$options_with_args $boolean_options";
    if (do {
__docker_complete_log_driver_options();
        $CHILD_ERROR == 0
    }) {
        return;
    }
    if (do {
__docker_complete_restart();
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $key = do {
    my ($in_172, $out_172);
    my $pid_172 = open3($in_172, $out_172, '>&STDERR', '__docker_map_key_of_current_option', '--security-opt');
    close $in_172 or croak 'Close failed: $OS_ERROR';
    my $result_172 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_172> };
    close $out_172 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_172, 0;
    $result_172
};
    my $COMPREPLY;
if ("$key" eq 'label') {
                if ($cur =~ /[*]:/msx) {
            return;
            $CHILD_ERROR = 0;
        } else {
            $CHILD_ERROR = 1;
        }
                @COMPREPLY = (do {
    my ($in_173, $out_173);
    my $pid_173 = open3($in_173, $out_173, '>&STDERR', 'compgen', '-W', "user: role: type: level: disable", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_173 or croak 'Close failed: $OS_ERROR';
    my $result_173 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_173> };
    close $out_173 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_173, 0;
    $result_173
});
        if ("${COMPREPLY[*]}" ne "disable") {
            __docker_nospace();
        }
        return;    } elsif ("$key" eq 'seccomp') {
                my $cur = ($ENV{cur} // q{}) =~ s/^.*=//sr;
                $main_exit_code = system('bash', '_filedir') >> 8;
                push @COMPREPLY, do {
    my ($in_174, $out_174);
    my $pid_174 = open3($in_174, $out_174, '>&STDERR', 'compgen', '-W', "unconfined", '--', "$cur");
    close $in_174 or croak 'Close failed: $OS_ERROR';
    my $result_174 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_174> };
    close $out_174 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_174, 0;
    $result_174
};
        return;    }
;
if ("$ENV{prev}" eq '--add-host') {
        if ("$cur" =~ /^.*:$/msx) {
                        __docker_complete_resolved_hostname();
            return;        }
    } elsif ("$ENV{prev}" eq '--attach' or "$ENV{prev}" eq '-a') {
                @COMPREPLY = (do {
    my ($in_175, $out_175);
    my $pid_175 = open3($in_175, $out_175, '>&STDERR', 'compgen', '-W', 'stdin stdout stderr', '--', "$cur");
    close $in_175 or croak 'Close failed: $OS_ERROR';
    my $result_175 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_175> };
    close $out_175 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_175, 0;
    $result_175
});
        return;    } elsif ("$ENV{prev}" eq '--cap-add') {
                __docker_complete_capabilities_addable();
        return;    } elsif ("$ENV{prev}" eq '--cap-drop') {
                __docker_complete_capabilities_droppable();
        return;    } elsif ("$ENV{prev}" eq '--cidfile' or "$ENV{prev}" eq '--env-file' or "$ENV{prev}" eq '--label-file') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--cgroupns') {
                @COMPREPLY = (do {
    my ($in_176, $out_176);
    my $pid_176 = open3($in_176, $out_176, '>&STDERR', 'compgen', '-W', "host private", '--', "$cur");
    close $in_176 or croak 'Close failed: $OS_ERROR';
    my $result_176 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_176> };
    close $out_176 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_176, 0;
    $result_176
});
        return;    } elsif ("$ENV{prev}" eq '--device' or "$ENV{prev}" eq '--tmpfs' or "$ENV{prev}" eq '--volume' or "$ENV{prev}" eq '-v') {
        if ("$cur" =~ /^.*:.*$/msx) {
        } elsif ("$cur" eq '') {
                        @COMPREPLY = (do {
    my ($in_177, $out_177);
    my $pid_177 = open3($in_177, $out_177, '>&STDERR', 'compgen', '-W', q{/}, '--', "$cur");
    close $in_177 or croak 'Close failed: $OS_ERROR';
    my $result_177 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_177> };
    close $out_177 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_177, 0;
    $result_177
});
                        __docker_nospace();
        } elsif ("$cur" =~ /^/.*$/msx) {
                        $main_exit_code = system('bash', '_filedir') >> 8;
                        __docker_nospace();
        }
        return;    } elsif ("$ENV{prev}" eq '--env' or "$ENV{prev}" eq '-e') {
                @COMPREPLY = (do {
    my ($in_178, $out_178);
    my $pid_178 = open3($in_178, $out_178, '>&STDERR', 'compgen', '-e', '--', "$cur");
    close $in_178 or croak 'Close failed: $OS_ERROR';
    my $result_178 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_178> };
    close $out_178 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_178, 0;
    $result_178
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--ipc') {
        if ("$cur" =~ /^.*:.*$/msx) {
                        $cur = (${cur} =~ s/^.*?://r =~ s/^.*?://r);
                        __docker_complete_containers_running();
        } elsif (1) {
                        @COMPREPLY = (do {
    my ($in_179, $out_179);
    my $pid_179 = open3($in_179, $out_179, '>&STDERR', 'compgen', '-W', 'none host private shareable container:', '--', "$cur");
    close $in_179 or croak 'Close failed: $OS_ERROR';
    my $result_179 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_179> };
    close $out_179 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_179, 0;
    $result_179
});
            if ("${COMPREPLY[*]}" eq "container:") {
                __docker_nospace();
            }
        }
        return;    } elsif ("$ENV{prev}" eq '--isolation') {
        if (!(        __docker_server_os_is('windows'))) {
            __docker_complete_isolation();
return;
        }
    } elsif ("$ENV{prev}" eq '--link') {
        if ("$cur" =~ /^.*:.*$/msx) {
        } elsif (1) {
                        __docker_complete_containers_running();
                        @COMPREPLY = (do {
    my ($in_180, $out_180);
    my $pid_180 = open3($in_180, $out_180, '>&STDERR', 'compgen', '-W', $COMPREPLY[eval { int(*) } // ""], '-S', q{:});
    close $in_180 or croak 'Close failed: $OS_ERROR';
    my $result_180 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_180> };
    close $out_180 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_180, 0;
    $result_180
});
                        __docker_nospace();
        }
        return;    } elsif ("$ENV{prev}" eq '--log-driver') {
                __docker_complete_log_drivers();
        return;    } elsif ("$ENV{prev}" eq '--log-opt') {
                __docker_complete_log_options();
        return;    } elsif ("$ENV{prev}" eq '--network') {
        if ("$cur" =~ /^container:.*$/msx) {
                        __docker_complete_containers_all('--cur', (${cur} =~ s/^.*?://r =~ s/^.*?://r));
        } elsif (1) {
                        @COMPREPLY = (do {
    my ($in_183, $out_183);
    my $pid_183 = open3($in_183, $out_183, '>&STDERR', 'compgen', '-W', (do {
    my ($in_181, $out_181);
    my $pid_181 = open3($in_181, $out_181, '>&STDERR', '__docker_plugins_bundled', '--type', 'Network');
    close $in_181 or croak 'Close failed: $OS_ERROR';
    my $result_181 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_181> };
    close $out_181 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_181, 0;
    $result_181
}) . " " . (do {
    my ($in_182, $out_182);
    my $pid_182 = open3($in_182, $out_182, '>&STDERR', '__docker_networks');
    close $in_182 or croak 'Close failed: $OS_ERROR';
    my $result_182 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_182> };
    close $out_182 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_182, 0;
    $result_182
}) . " container:", '--', "$cur");
    close $in_183 or croak 'Close failed: $OS_ERROR';
    my $result_183 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_183> };
    close $out_183 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_183, 0;
    $result_183
});
            if ("${COMPREPLY[*]}" eq "container:") {
                __docker_nospace();
            }
        }
        return;    } elsif ("$ENV{prev}" eq '--pid') {
        if ("$cur" =~ /^.*:.*$/msx) {
                        __docker_complete_containers_running('--cur', (${cur} =~ s/^.*?://r =~ s/^.*?://r));
        } elsif (1) {
                        @COMPREPLY = (do {
    my ($in_184, $out_184);
    my $pid_184 = open3($in_184, $out_184, '>&STDERR', 'compgen', '-W', 'host container:', '--', "$cur");
    close $in_184 or croak 'Close failed: $OS_ERROR';
    my $result_184 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_184> };
    close $out_184 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_184, 0;
    $result_184
});
            if ("${COMPREPLY[*]}" eq "container:") {
                __docker_nospace();
            }
        }
        return;    } elsif ("$ENV{prev}" eq '--pull') {
                @COMPREPLY = (do {
    my ($in_185, $out_185);
    my $pid_185 = open3($in_185, $out_185, '>&STDERR', 'compgen', '-W', 'always missing never', '--', "$cur");
    close $in_185 or croak 'Close failed: $OS_ERROR';
    my $result_185 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_185> };
    close $out_185 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_185, 0;
    $result_185
});
        return;    } elsif ("$ENV{prev}" eq '--runtime') {
                __docker_complete_runtimes();
        return;    } elsif ("$ENV{prev}" eq '--security-opt') {
                @COMPREPLY = (do {
    my ($in_186, $out_186);
    my $pid_186 = open3($in_186, $out_186, '>&STDERR', 'compgen', '-W', "apparmor= label= no-new-privileges seccomp= " . "sys" . "tem" . "paths=unconfined", '--', "$cur");
    close $in_186 or croak 'Close failed: $OS_ERROR';
    my $result_186 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_186> };
    close $out_186 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_186, 0;
    $result_186
});
        if (0) {
            __docker_nospace();
        }
        return;    } elsif ("$ENV{prev}" eq '--stop-signal') {
                __docker_complete_signals();
        return;    } elsif ("$ENV{prev}" eq '--storage-opt') {
                @COMPREPLY = (do {
    my ($in_187, $out_187);
    my $pid_187 = open3($in_187, $out_187, '>&STDERR', 'compgen', '-W', "size", '-S', q{=}, '--', "$cur");
    close $in_187 or croak 'Close failed: $OS_ERROR';
    my $result_187 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_187> };
    close $out_187 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_187, 0;
    $result_187
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--ulimit') {
                __docker_complete_ulimits();
        return;    } elsif ("$ENV{prev}" eq '--user' or "$ENV{prev}" eq '-u') {
                __docker_complete_user_group();
        return;    } elsif ("$ENV{prev}" eq '--userns') {
                @COMPREPLY = (do {
    my ($in_188, $out_188);
    my $pid_188 = open3($in_188, $out_188, '>&STDERR', 'compgen', '-W', "host", '--', "$cur");
    close $in_188 or croak 'Close failed: $OS_ERROR';
    my $result_188 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_188> };
    close $out_188 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_188, 0;
    $result_188
});
        return;    } elsif ("$ENV{prev}" eq '--volume-driver') {
                __docker_complete_plugins_bundled('--type', 'Volume');
        return;    } elsif ("$ENV{prev}" eq '--volumes-from') {
                __docker_complete_containers_all();
        return;    } elsif ("$ENV{prev}" eq '$(__docker_to_extglob "$options_with_args")') {
        return;    }
if ("$cur" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_189, $out_189);
    my $pid_189 = open3($in_189, $out_189, '>&STDERR', 'compgen', '-W', "$all_options", '--', "$cur");
    close $in_189 or croak 'Close failed: $OS_ERROR';
    my $result_189 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_189> };
    close $out_189 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_189, 0;
    $result_189
});
    } elsif (1) {
                my $counter = do {
    my ($in_191, $out_191);
    my $pid_191 = open3($in_191, $out_191, '>&STDERR', '__docker_pos_first_nonflag', (do {
    my ($in_190, $out_190);
    my $pid_190 = open3($in_190, $out_190, '>&STDERR', '__docker_to_alternatives', "$options_with_args");
    close $in_190 or croak 'Close failed: $OS_ERROR';
    my $result_190 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_190> };
    close $out_190 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_190, 0;
    $result_190
}));
    close $in_191 or croak 'Close failed: $OS_ERROR';
    my $result_191 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_191> };
    close $out_191 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_191, 0;
    $result_191
};
        if (($cword == $counter)) {
            __docker_complete_images('--repo', '--tag', '--id');
        }
    }
    return;
}

sub _docker_container_start {
    if (do {
__docker_complete_detach_keys();
        $CHILD_ERROR == 0
    }) {
        return;
    }
if ("$ENV{prev}" eq '--checkpoint') {
        if (!(        __docker_server_is_experimental())) {
return;
        }
    } elsif ("$ENV{prev}" eq '--checkpoint-dir') {
        if (!(        __docker_server_is_experimental())) {
            $main_exit_code = system('_filedir', '-d') >> 8;
return;
        }
    }
    my $options;
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                my $options = "--attach -a --detach-keys --help --interactive -i";
                if (do {
__docker_server_is_experimental();
            $CHILD_ERROR == 0
        }) {
                        my $checkpoint;
            my $dir;
            $options = eval { int($options+" --$checkpoint --$checkpoint-$dir") } // "";
        }
                @COMPREPLY = (do {
    my ($in_192, $out_192);
    my $pid_192 = open3($in_192, $out_192, '>&STDERR', 'compgen', '-W', "$options", '--', "$ENV{cur}");
    close $in_192 or croak 'Close failed: $OS_ERROR';
    my $result_192 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_192> };
    close $out_192 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_192, 0;
    $result_192
});
    } elsif (1) {
                __docker_complete_containers_stopped();
    }
;
    return;
}

sub _docker_container_stats {
if ("$ENV{prev}" eq '--format') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_193, $out_193);
    my $pid_193 = open3($in_193, $out_193, '>&STDERR', 'compgen', '-W', "--all -a --format --help --no-stream --no-trunc", '--', "$ENV{cur}");
    close $in_193 or croak 'Close failed: $OS_ERROR';
    my $result_193 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_193> };
    close $out_193 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_193, 0;
    $result_193
});
    } elsif (1) {
                __docker_complete_containers_running();
    }
;
    return;
}

sub _docker_container_stop {
if ("$ENV{prev}" eq '--time' or "$ENV{prev}" eq '-t') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_194, $out_194);
    my $pid_194 = open3($in_194, $out_194, '>&STDERR', 'compgen', '-W', "--help --time -t", '--', "$ENV{cur}");
    close $in_194 or croak 'Close failed: $OS_ERROR';
    my $result_194 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_194> };
    close $out_194 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_194, 0;
    $result_194
});
    } elsif (1) {
                __docker_complete_containers_stoppable();
    }
;
    return;
}

sub _docker_container_top {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_195, $out_195);
    my $pid_195 = open3($in_195, $out_195, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_195 or croak 'Close failed: $OS_ERROR';
    my $result_195 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_195> };
    close $out_195 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_195, 0;
    $result_195
});
    } elsif (1) {
                my $counter = do {
    my ($in_196, $out_196);
    my $pid_196 = open3($in_196, $out_196, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_196 or croak 'Close failed: $OS_ERROR';
    my $result_196 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_196> };
    close $out_196 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_196, 0;
    $result_196
};
        if (($cword == $counter)) {
            __docker_complete_containers_running();
        }
    }
;
    return;
}

sub _docker_container_unpause {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_197, $out_197);
    my $pid_197 = open3($in_197, $out_197, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_197 or croak 'Close failed: $OS_ERROR';
    my $result_197 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_197> };
    close $out_197 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_197, 0;
    $result_197
});
    } elsif (1) {
                my $counter = do {
    my ($in_198, $out_198);
    my $pid_198 = open3($in_198, $out_198, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_198 or croak 'Close failed: $OS_ERROR';
    my $result_198 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_198> };
    close $out_198 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_198, 0;
    $result_198
};
        if (($cword == $counter)) {
            __docker_complete_containers_unpauseable();
        }
    }
;
    return;
}

sub _docker_container_update {
    my $options_with_args = "
		--blkio-weight
		--cpu-period
		--cpu-quota
		--cpu-rt-period
		--cpu-rt-runtime
		--cpus
		--cpuset-cpus
		--cpuset-mems
		--cpu-shares -c
		--kernel-memory
		--memory -m
		--memory-reservation
		--memory-swap
		--pids-limit
		--restart
	";
    my $boolean_options = "
		--help
	";
    my $all_options = "$options_with_args $boolean_options";
    if (do {
__docker_complete_restart();
        $CHILD_ERROR == 0
    }) {
        return;
    }
if ("$ENV{prev}" eq '$(__docker_to_extglob "$options_with_args")') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_199, $out_199);
    my $pid_199 = open3($in_199, $out_199, '>&STDERR', 'compgen', '-W', "$all_options", '--', "$ENV{cur}");
    close $in_199 or croak 'Close failed: $OS_ERROR';
    my $result_199 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_199> };
    close $out_199 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_199, 0;
    $result_199
});
    } elsif (1) {
                __docker_complete_containers_all();
    }
;
    return;
}

sub _docker_container_wait {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_200, $out_200);
    my $pid_200 = open3($in_200, $out_200, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_200 or croak 'Close failed: $OS_ERROR';
    my $result_200 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_200> };
    close $out_200 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_200, 0;
    $result_200
});
    } elsif (1) {
                __docker_complete_containers_all();
    }
;
    return;
}

sub _docker_context {
    my $subcommands = "
		create
		export
		import
		inspect
		ls
		rm
		update
		use
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_201, $out_201);
    my $pid_201 = open3($in_201, $out_201, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_201 or croak 'Close failed: $OS_ERROR';
    my $result_201 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_201> };
    close $out_201 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_201, 0;
    $result_201
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_202, $out_202);
    my $pid_202 = open3($in_202, $out_202, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_202 or croak 'Close failed: $OS_ERROR';
    my $result_202 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_202> };
    close $out_202 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_202, 0;
    $result_202
});
    }
;
    return;
}

sub _docker_context_create {
if ("$ENV{prev}" eq '--description' or "$ENV{prev}" eq '--docker') {
        return;    } elsif ("$ENV{prev}" eq '--from') {
                __docker_complete_contexts();
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_203, $out_203);
    my $pid_203 = open3($in_203, $out_203, '>&STDERR', 'compgen', '-W', "--description --docker --from --help", '--', "$ENV{cur}");
    close $in_203 or croak 'Close failed: $OS_ERROR';
    my $result_203 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_203> };
    close $out_203 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_203, 0;
    $result_203
});
    }
;
    return;
}

sub _docker_context_export {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_204, $out_204);
    my $pid_204 = open3($in_204, $out_204, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_204 or croak 'Close failed: $OS_ERROR';
    my $result_204 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_204> };
    close $out_204 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_204, 0;
    $result_204
});
    } elsif (1) {
                my $counter = do {
    my ($in_205, $out_205);
    my $pid_205 = open3($in_205, $out_205, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_205 or croak 'Close failed: $OS_ERROR';
    my $result_205 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_205> };
    close $out_205 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_205, 0;
    $result_205
};
        if (($cword == $counter)) {
            __docker_complete_contexts();
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                $main_exit_code = system('bash', '_filedir') >> 8;
            }
        }
    }
;
    return;
}

sub _docker_context_import {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_206, $out_206);
    my $pid_206 = open3($in_206, $out_206, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_206 or croak 'Close failed: $OS_ERROR';
    my $result_206 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_206> };
    close $out_206 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_206, 0;
    $result_206
});
    } elsif (1) {
                my $counter = do {
    my ($in_207, $out_207);
    my $pid_207 = open3($in_207, $out_207, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_207 or croak 'Close failed: $OS_ERROR';
    my $result_207 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_207> };
    close $out_207 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_207, 0;
    $result_207
};
        if (($cword == $counter)) {
            $main_exit_code = system('bash', ':') >> 8;
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                $main_exit_code = system('bash', '_filedir') >> 8;
            }
        }
    }
;
    return;
}

sub _docker_context_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_208, $out_208);
    my $pid_208 = open3($in_208, $out_208, '>&STDERR', 'compgen', '-W', "--format -f --help", '--', "$ENV{cur}");
    close $in_208 or croak 'Close failed: $OS_ERROR';
    my $result_208 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_208> };
    close $out_208 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_208, 0;
    $result_208
});
    } elsif (1) {
                __docker_complete_contexts();
    }
;
    return;
}

sub _docker_context_list {
    $main_exit_code = system('bash', '_docker_context_ls') >> 8;
    return;
}

sub _docker_context_ls {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_209, $out_209);
    my $pid_209 = open3($in_209, $out_209, '>&STDERR', 'compgen', '-W', "--format -f --help --quiet -q", '--', "$ENV{cur}");
    close $in_209 or croak 'Close failed: $OS_ERROR';
    my $result_209 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_209> };
    close $out_209 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_209, 0;
    $result_209
});
    }
;
    return;
}

sub _docker_context_remove {
    $main_exit_code = system('bash', '_docker_context_rm') >> 8;
    return;
}

sub _docker_context_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_210, $out_210);
    my $pid_210 = open3($in_210, $out_210, '>&STDERR', 'compgen', '-W', "--force -f --help", '--', "$ENV{cur}");
    close $in_210 or croak 'Close failed: $OS_ERROR';
    my $result_210 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_210> };
    close $out_210 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_210, 0;
    $result_210
});
    } elsif (1) {
                __docker_complete_contexts();
    }
;
    return;
}

sub _docker_context_update {
if ("$ENV{prev}" eq '--description' or "$ENV{prev}" eq '--docker') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_211, $out_211);
    my $pid_211 = open3($in_211, $out_211, '>&STDERR', 'compgen', '-W', "--description --docker --help", '--', "$ENV{cur}");
    close $in_211 or croak 'Close failed: $OS_ERROR';
    my $result_211 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_211> };
    close $out_211 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_211, 0;
    $result_211
});
    } elsif (1) {
                my $counter = do {
    my ($in_212, $out_212);
    my $pid_212 = open3($in_212, $out_212, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_212 or croak 'Close failed: $OS_ERROR';
    my $result_212 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_212> };
    close $out_212 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_212, 0;
    $result_212
};
        if (($cword == $counter)) {
            __docker_complete_contexts();
        }
    }
;
    return;
}

sub _docker_context_use {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_213, $out_213);
    my $pid_213 = open3($in_213, $out_213, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_213 or croak 'Close failed: $OS_ERROR';
    my $result_213 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_213> };
    close $out_213 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_213, 0;
    $result_213
});
    } elsif (1) {
                my $counter = do {
    my ($in_214, $out_214);
    my $pid_214 = open3($in_214, $out_214, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_214 or croak 'Close failed: $OS_ERROR';
    my $result_214 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_214> };
    close $out_214 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_214, 0;
    $result_214
};
        if (($cword == $counter)) {
            __docker_complete_contexts('--add', 'default');
        }
    }
;
    return;
}

sub _docker_commit {
    _docker_container_commit();
    return;
}

sub _docker_cp {
    _docker_container_cp();
    return;
}

sub _docker_create {
    _docker_container_create();
    return;
}

sub _docker_daemon {
    my $boolean_options = "
		$ENV{global_boolean_options}
		--experimental
		--help
		--icc=false
		--init
		--ip-forward=false
		--ip-masq=false
		--iptables=false
		--ip6tables
		--ipv6
		--live-restore
		--no-new-privileges
		--raw-logs
		--selinux-enabled
		--userland-proxy=false
		--validate
		--version -v
	";
    my $options_with_args = "
		$ENV{global_options_with_args}
		--add-runtime
		--allow-nondistributable-artifacts
		--api-cors-header
		--authorization-plugin
		--bip
		--bridge -b
		--cgroup-parent
		--config-file
		--containerd
		--containerd-namespace
		--containerd-plugins-namespace
		--cpu-rt-period
		--cpu-rt-runtime
		--data-root
		--default-address-pool
		--default-gateway
		--default-gateway-v6
		--default-runtime
		--default-shm-size
		--default-ulimit
		--dns
		--dns-search
		--dns-opt
		--exec-opt
		--exec-root
		--fixed-cidr
		--fixed-cidr-v6
		--group -G
		--init-path
		--insecure-registry
		--ip
		--label
		--log-driver
		--log-opt
		--max-concurrent-downloads
		--max-concurrent-uploads
		--max-download-attempts
		--metrics-addr
		--mtu
		--network-control-plane-mtu
		--node-generic-resource
		--oom-score-adjust
		--pidfile -p
		--registry-mirror
		--seccomp-profile
		--shutdown-timeout
		--storage-driver -s
		--storage-opt
		--swarm-default-advertise-addr
		--userland-proxy-path
		--userns-remap
	";
    if (do {
__docker_complete_log_driver_options();
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $key = do {
    my ($in_215, $out_215);
    my $pid_215 = open3($in_215, $out_215, '>&STDERR', '__docker_map_key_of_current_option', '--storage-opt');
    close $in_215 or croak 'Close failed: $OS_ERROR';
    my $result_215 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_215> };
    close $out_215 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_215, 0;
    $result_215
};
    my $COMPREPLY;
    my $cur;
if ("$key" eq 'dm.blkdiscard' or "$key" eq 'dm.override_udev_sync_check' or "$key" eq 'dm.use_deferred_removal' or "$key" eq 'dm.use_deferred_deletion') {
                @COMPREPLY = (do {
    my ($in_216, $out_216);
    my $pid_216 = open3($in_216, $out_216, '>&STDERR', 'compgen', '-W', "false true", '--', (${cur} =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_216 or croak 'Close failed: $OS_ERROR';
    my $result_216 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_216> };
    close $out_216 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_216, 0;
    $result_216
});
        return;    } elsif ("$key" eq 'dm.directlvm_device' or "$key" eq 'dm.thinpooldev') {
                $cur = ${cur} =~ s/^.*=//sr;
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    } elsif ("$key" eq 'dm.fs') {
                @COMPREPLY = (do {
    my ($in_217, $out_217);
    my $pid_217 = open3($in_217, $out_217, '>&STDERR', 'compgen', '-W', "ext4 xfs", '--', (${cur} =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_217 or croak 'Close failed: $OS_ERROR';
    my $result_217 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_217> };
    close $out_217 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_217, 0;
    $result_217
});
        return;    } elsif ("$key" eq 'dm.libdm_log_level') {
                @COMPREPLY = (do {
    my ($in_218, $out_218);
    my $pid_218 = open3($in_218, $out_218, '>&STDERR', 'compgen', '-W', "2 3 4 5 6 7", '--', (${cur} =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_218 or croak 'Close failed: $OS_ERROR';
    my $result_218 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_218> };
    close $out_218 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_218, 0;
    $result_218
});
        return;    }
;
if ("$ENV{prev}" eq '--authorization-plugin') {
                __docker_complete_plugins_bundled('--type', 'Authorization');
        return;    } elsif ("$ENV{prev}" eq '--config-file' or "$ENV{prev}" eq '--containerd' or "$ENV{prev}" eq '--init-path' or "$ENV{prev}" eq '--pidfile' or "$ENV{prev}" eq '-p' or "$ENV{prev}" eq '--tlscacert' or "$ENV{prev}" eq '--tlscert' or "$ENV{prev}" eq '--tlskey' or "$ENV{prev}" eq '--userland-proxy-path') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--default-ulimit') {
                __docker_complete_ulimits();
        return;    } elsif ("$ENV{prev}" eq '--exec-root' or "$ENV{prev}" eq '--data-root') {
                $main_exit_code = system('_filedir', '-d') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--log-driver') {
                __docker_complete_log_drivers();
        return;    } elsif ("$ENV{prev}" eq '--storage-driver' or "$ENV{prev}" eq '-s') {
                @COMPREPLY = ('$( compgen -W "btrfs overlay2 vfs zfs" -- "$(echo "$cur" | tr \'[:upper:]\' \'[:lower:]\')"');
        return;    } elsif ("$ENV{prev}" eq '--storage-opt') {
                my $btrfs_options = "btrfs.min_space";
                my $overlay2_options = "overlay2.size";
                my $zfs_options = "zfs.fsname";
                my $all_options = "$btrfs_options $overlay2_options $zfs_options";
        if (do {
    my ($in_221, $out_221);
    my $pid_221 = open3($in_221, $out_221, '>&STDERR', '__docker_value_of_option', '--storage-driver|-s');
    close $in_221 or croak 'Close failed: $OS_ERROR';
    my $result_221 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_221> };
    close $out_221 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_221, 0;
    $result_221
} eq '') {
                        @COMPREPLY = (do {
    my ($in_222, $out_222);
    my $pid_222 = open3($in_222, $out_222, '>&STDERR', 'compgen', '-W', "$all_options", '-S', q{=}, '--', "$cur");
    close $in_222 or croak 'Close failed: $OS_ERROR';
    my $result_222 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_222> };
    close $out_222 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_222, 0;
    $result_222
});
        } elsif (do {
    my ($in_223, $out_223);
    my $pid_223 = open3($in_223, $out_223, '>&STDERR', '__docker_value_of_option', '--storage-driver|-s');
    close $in_223 or croak 'Close failed: $OS_ERROR';
    my $result_223 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_223> };
    close $out_223 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_223, 0;
    $result_223
} eq 'btrfs') {
                        @COMPREPLY = (do {
    my ($in_224, $out_224);
    my $pid_224 = open3($in_224, $out_224, '>&STDERR', 'compgen', '-W', "$btrfs_options", '-S', q{=}, '--', "$cur");
    close $in_224 or croak 'Close failed: $OS_ERROR';
    my $result_224 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_224> };
    close $out_224 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_224, 0;
    $result_224
});
        } elsif (do {
    my ($in_225, $out_225);
    my $pid_225 = open3($in_225, $out_225, '>&STDERR', '__docker_value_of_option', '--storage-driver|-s');
    close $in_225 or croak 'Close failed: $OS_ERROR';
    my $result_225 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_225> };
    close $out_225 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_225, 0;
    $result_225
} eq 'overlay2') {
                        @COMPREPLY = (do {
    my ($in_226, $out_226);
    my $pid_226 = open3($in_226, $out_226, '>&STDERR', 'compgen', '-W', "$overlay2_options", '-S', q{=}, '--', "$cur");
    close $in_226 or croak 'Close failed: $OS_ERROR';
    my $result_226 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_226> };
    close $out_226 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_226, 0;
    $result_226
});
        } elsif (do {
    my ($in_227, $out_227);
    my $pid_227 = open3($in_227, $out_227, '>&STDERR', '__docker_value_of_option', '--storage-driver|-s');
    close $in_227 or croak 'Close failed: $OS_ERROR';
    my $result_227 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_227> };
    close $out_227 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_227, 0;
    $result_227
} eq 'zfs') {
                        @COMPREPLY = (do {
    my ($in_228, $out_228);
    my $pid_228 = open3($in_228, $out_228, '>&STDERR', 'compgen', '-W', "$zfs_options", '-S', q{=}, '--', "$cur");
    close $in_228 or croak 'Close failed: $OS_ERROR';
    my $result_228 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_228> };
    close $out_228 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_228, 0;
    $result_228
});
        } elsif (1) {
            return;        }
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--log-level' or "$ENV{prev}" eq '-l') {
                __docker_complete_log_levels();
        return;    } elsif ("$ENV{prev}" eq '--log-opt') {
                __docker_complete_log_options();
        return;    } elsif ("$ENV{prev}" eq '--metrics-addr') {
                __docker_complete_local_ips();
                __docker_append_to_completions(":");
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--seccomp-profile') {
                $main_exit_code = system('_filedir', 'json') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--swarm-default-advertise-addr') {
                __docker_complete_local_interfaces();
        return;    } elsif ("$ENV{prev}" eq '--userns-remap') {
                __docker_complete_user_group();
        return;    } elsif ("$ENV{prev}" eq '$(__docker_to_extglob "$options_with_args")') {
        return;    }
if ("$cur" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_229, $out_229);
    my $pid_229 = open3($in_229, $out_229, '>&STDERR', 'compgen', '-W', "$boolean_options $options_with_args", '--', "$cur");
    close $in_229 or croak 'Close failed: $OS_ERROR';
    my $result_229 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_229> };
    close $out_229 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_229, 0;
    $result_229
});
    }
    return;
}

sub _docker_diff {
    _docker_container_diff();
    return;
}

sub _docker_events {
    $main_exit_code = system('bash', '_docker_system_events') >> 8;
    return;
}

sub _docker_exec {
    _docker_container_exec();
    return;
}

sub _docker_export {
    _docker_container_export();
    return;
}

sub _docker_help {
    my $counter = do {
    my ($in_230, $out_230);
    my $pid_230 = open3($in_230, $out_230, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_230 or croak 'Close failed: $OS_ERROR';
    my $result_230 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_230> };
    close $out_230 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_230, 0;
    $result_230
};
    my $COMPREPLY;
if (($cword == $counter)) {
        @COMPREPLY = (do {
    my ($in_231, $out_231);
    my $pid_231 = open3($in_231, $out_231, '>&STDERR', 'compgen', '-W', $commands[eval { int(*) } // ""], '--', "$ENV{cur}");
    close $in_231 or croak 'Close failed: $OS_ERROR';
    my $result_231 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_231> };
    close $out_231 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_231, 0;
    $result_231
});
    }
;
    return;
}

sub _docker_history {
    $main_exit_code = system('bash', '_docker_image_history') >> 8;
    return;
}

sub _docker_image {
    my $subcommands = "
		build
		history
		import
		inspect
		load
		ls
		prune
		pull
		push
		rm
		save
		tag
	";
    my $aliases = "
		images
		list
		remove
		rmi
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_232, $out_232);
    my $pid_232 = open3($in_232, $out_232, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_232 or croak 'Close failed: $OS_ERROR';
    my $result_232 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_232> };
    close $out_232 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_232, 0;
    $result_232
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_233, $out_233);
    my $pid_233 = open3($in_233, $out_233, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_233 or croak 'Close failed: $OS_ERROR';
    my $result_233 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_233> };
    close $out_233 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_233, 0;
    $result_233
});
    }
;
    return;
}

sub _docker_image_build {
    my $options_with_args = "
		--add-host
		--build-arg
		--cache-from
		--cgroup-parent
		--cpuset-cpus
		--cpuset-mems
		--cpu-shares -c
		--cpu-period
		--cpu-quota
		--file -f
		--iidfile
		--label
		--memory -m
		--memory-swap
		--network
		--platform
		--shm-size
		--tag -t
		--target
		--ulimit
	";
    if (do {
__docker_server_os_is('windows');
        $CHILD_ERROR == 0
    }) {
                my $isolation;
        $options_with_args = eval { int($options_with_args+"
		--$isolation
	") } // "";
    }
    my $boolean_options = "
		--disable-content-trust=false
		--force-rm
		--help
		--no-cache
		--pull
		--quiet -q
		--rm
	";
if (!(    __docker_server_is_experimental())) {
        $boolean_options = "
			--squash
		";
    }
if ("${DOCKER_BUILDKIT-}" eq "1") {
        $options_with_args = "
			--output -o
			--progress
			--secret
			--ssh
		";
}
    else {
        $boolean_options = "
			--compress
		";
    }
    my $all_options = "$options_with_args $boolean_options";
    my $context;
    my $COMPREPLY;
if ("$ENV{prev}" eq '--add-host') {
        if ("$ENV{cur}" =~ /^.*:$/msx) {
                        __docker_complete_resolved_hostname();
            return;        }
    } elsif ("$ENV{prev}" eq '--build-arg') {
                @COMPREPLY = (do {
    my ($in_234, $out_234);
    my $pid_234 = open3($in_234, $out_234, '>&STDERR', 'compgen', '-e', '--', "$ENV{cur}");
    close $in_234 or croak 'Close failed: $OS_ERROR';
    my $result_234 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_234> };
    close $out_234 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_234, 0;
    $result_234
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--cache-from') {
                __docker_complete_images('--repo', '--tag', '--id');
        return;    } elsif ("$ENV{prev}" eq '--file' or "$ENV{prev}" eq '-f' or "$ENV{prev}" eq '--iidfile') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--isolation') {
        if (!(        __docker_server_os_is('windows'))) {
            __docker_complete_isolation();
return;
        }
    } elsif ("$ENV{prev}" eq '--network') {
        if ("$ENV{cur}" =~ /^container:.*$/msx) {
                        __docker_complete_containers_all('--cur', (($ENV{cur} // q{}) =~ s/^.*?://r =~ s/^.*?://r));
        } elsif (1) {
                        @COMPREPLY = (do {
    my ($in_237, $out_237);
    my $pid_237 = open3($in_237, $out_237, '>&STDERR', 'compgen', '-W', (do {
    my ($in_235, $out_235);
    my $pid_235 = open3($in_235, $out_235, '>&STDERR', '__docker_plugins_bundled', '--type', 'Network');
    close $in_235 or croak 'Close failed: $OS_ERROR';
    my $result_235 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_235> };
    close $out_235 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_235, 0;
    $result_235
}) . " " . (do {
    my ($in_236, $out_236);
    my $pid_236 = open3($in_236, $out_236, '>&STDERR', '__docker_networks');
    close $in_236 or croak 'Close failed: $OS_ERROR';
    my $result_236 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_236> };
    close $out_236 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_236, 0;
    $result_236
}) . " container:", '--', "$ENV{cur}");
    close $in_237 or croak 'Close failed: $OS_ERROR';
    my $result_237 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_237> };
    close $out_237 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_237, 0;
    $result_237
});
            if ("${COMPREPLY[*]}" eq "container:") {
                __docker_nospace();
            }
        }
        return;    } elsif ("$ENV{prev}" eq '--progress') {
                @COMPREPLY = (do {
    my ($in_238, $out_238);
    my $pid_238 = open3($in_238, $out_238, '>&STDERR', 'compgen', '-W', "auto plain tty", '--', "$ENV{cur}");
    close $in_238 or croak 'Close failed: $OS_ERROR';
    my $result_238 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_238> };
    close $out_238 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_238, 0;
    $result_238
});
        return;    } elsif ("$ENV{prev}" eq '--tag' or "$ENV{prev}" eq '-t') {
                __docker_complete_images('--repo', '--tag');
        return;    } elsif ("$ENV{prev}" eq '--target') {
                my $context_pos = do {
    my ($in_240, $out_240);
    my $pid_240 = open3($in_240, $out_240, '>&STDERR', '__docker_pos_first_nonflag', (do {
    my ($in_239, $out_239);
    my $pid_239 = open3($in_239, $out_239, '>&STDERR', '__docker_to_alternatives', "$options_with_args");
    close $in_239 or croak 'Close failed: $OS_ERROR';
    my $result_239 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_239> };
    close $out_239 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_239, 0;
    $result_239
}));
    close $in_240 or croak 'Close failed: $OS_ERROR';
    my $result_240 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_240> };
    close $out_240 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_240, 0;
    $result_240
};
                my $context = $words[eval { int($context_pos) } // ""];
                $context = (defined (defined ${context} && ${context} ne q{} ? ${context} : '.') && (defined ${context} && ${context} ne q{} ? ${context} : '.') ne q{} ? (defined ${context} && ${context} ne q{} ? ${context} : '.') : '.');
                my $file = (do {
    my ($in_241, $out_241);
    my $pid_241 = open3($in_241, $out_241, '>&STDERR', '__docker_value_of_option', '--file|f');
    close $in_241 or croak 'Close failed: $OS_ERROR';
    my $result_241 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_241> };
    close $out_241 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_241, 0;
    $result_241
});
                my $default_file = (scalar reverse( (scalar reverse ${context}) =~ s/^///r ) =~ s//$//r) . "/Dockerfile";
                my $dockerfile = (defined (defined ${file} && ${file} ne q{} ? ${file} : '$default_file') && (defined ${file} && ${file} ne q{} ? ${file} : '$default_file') ne q{} ? (defined ${file} && ${file} ne q{} ? ${file} : '$default_file') : '$default_file');
                my $targets = (do { my @_qx_cmd = ("sed -n \"s/^FROM .\\\\+ AS \\\\(.\\\\+\\\\)/\\\\1/p\" \"$dockerfile\" 2> /dev/null"); chomp(my $result = qx{$_qx_cmd[0]}); $CHILD_ERROR = $? >> 8; $result; });
                @COMPREPLY = (do {
    my ($in_242, $out_242);
    my $pid_242 = open3($in_242, $out_242, '>&STDERR', 'compgen', '-W', "$targets", '--', "$ENV{cur}");
    close $in_242 or croak 'Close failed: $OS_ERROR';
    my $result_242 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_242> };
    close $out_242 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_242, 0;
    $result_242
});
        return;    } elsif ("$ENV{prev}" eq '--ulimit') {
                __docker_complete_ulimits();
        return;    } elsif ("$ENV{prev}" eq '$(__docker_to_extglob "$options_with_args")') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_243, $out_243);
    my $pid_243 = open3($in_243, $out_243, '>&STDERR', 'compgen', '-W', "$all_options", '--', "$ENV{cur}");
    close $in_243 or croak 'Close failed: $OS_ERROR';
    my $result_243 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_243> };
    close $out_243 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_243, 0;
    $result_243
});
    } elsif (1) {
                my $counter = do {
    my ($in_245, $out_245);
    my $pid_245 = open3($in_245, $out_245, '>&STDERR', '__docker_pos_first_nonflag', (do {
    my ($in_244, $out_244);
    my $pid_244 = open3($in_244, $out_244, '>&STDERR', '__docker_to_alternatives', "$options_with_args");
    close $in_244 or croak 'Close failed: $OS_ERROR';
    my $result_244 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_244> };
    close $out_244 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_244, 0;
    $result_244
}));
    close $in_245 or croak 'Close failed: $OS_ERROR';
    my $result_245 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_245> };
    close $out_245 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_245, 0;
    $result_245
};
        if (($cword == $counter)) {
            $main_exit_code = system('_filedir', '-d') >> 8;
        }
    }
    return;
}

sub _docker_image_history {
if ("$ENV{prev}" eq '--format') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_246, $out_246);
    my $pid_246 = open3($in_246, $out_246, '>&STDERR', 'compgen', '-W', "--format --help --human=false -H=false --no-trunc --quiet -q", '--', "$ENV{cur}");
    close $in_246 or croak 'Close failed: $OS_ERROR';
    my $result_246 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_246> };
    close $out_246 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_246, 0;
    $result_246
});
    } elsif (1) {
                my $counter = do {
    my ($in_247, $out_247);
    my $pid_247 = open3($in_247, $out_247, '>&STDERR', '__docker_pos_first_nonflag', '--format');
    close $in_247 or croak 'Close failed: $OS_ERROR';
    my $result_247 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_247> };
    close $out_247 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_247, 0;
    $result_247
};
        if (($cword == $counter)) {
            __docker_complete_images('--force-tag', '--id');
        }
    }
;
    return;
}

sub _docker_image_images {
    $main_exit_code = system('bash', '_docker_image_ls') >> 8;
    return;
}

sub _docker_image_import {
if ("$ENV{prev}" eq '--change' or "$ENV{prev}" eq '-c' or "$ENV{prev}" eq '--message' or "$ENV{prev}" eq '-m' or "$ENV{prev}" eq '--platform') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                my $options = "--change -c --help --message -m --platform";
                @COMPREPLY = (do {
    my ($in_248, $out_248);
    my $pid_248 = open3($in_248, $out_248, '>&STDERR', 'compgen', '-W', "$options", '--', "$ENV{cur}");
    close $in_248 or croak 'Close failed: $OS_ERROR';
    my $result_248 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_248> };
    close $out_248 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_248, 0;
    $result_248
});
    } elsif (1) {
                my $counter = do {
    my ($in_249, $out_249);
    my $pid_249 = open3($in_249, $out_249, '>&STDERR', '__docker_pos_first_nonflag', '--change|-c|--message|-m');
    close $in_249 or croak 'Close failed: $OS_ERROR';
    my $result_249 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_249> };
    close $out_249 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_249, 0;
    $result_249
};
        if (($cword == $counter)) {
            $main_exit_code = system('bash', '_filedir') >> 8;
return;
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                __docker_complete_images('--repo', '--tag');
return;
            }
        }
    }
;
    return;
}

sub _docker_image_inspect {
    $main_exit_code = system('_docker_inspect', '--type', 'image') >> 8;
    return;
}

sub _docker_image_load {
if ("$ENV{prev}" eq '--input' or "$ENV{prev}" eq '-i' or "$ENV{prev}" eq '<') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_250, $out_250);
    my $pid_250 = open3($in_250, $out_250, '>&STDERR', 'compgen', '-W', "--help --input -i --quiet -q", '--', "$ENV{cur}");
    close $in_250 or croak 'Close failed: $OS_ERROR';
    my $result_250 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_250> };
    close $out_250 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_250, 0;
    $result_250
});
    }
;
    return;
}

sub _docker_image_list {
    $main_exit_code = system('bash', '_docker_image_ls') >> 8;
    return;
}

sub _docker_image_ls {
    my $key = do {
    my ($in_251, $out_251);
    my $pid_251 = open3($in_251, $out_251, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_251 or croak 'Close failed: $OS_ERROR';
    my $result_251 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_251> };
    close $out_251 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_251, 0;
    $result_251
};
    my $COMPREPLY;
if ("$key" eq 'before' or "$key" eq 'since') {
                __docker_complete_images('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--force-tag', '--id');
        return;    } elsif ("$key" eq 'dangling') {
                @COMPREPLY = (do {
    my ($in_252, $out_252);
    my $pid_252 = open3($in_252, $out_252, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_252 or croak 'Close failed: $OS_ERROR';
    my $result_252 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_252> };
    close $out_252 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_252, 0;
    $result_252
});
        return;    } elsif ("$key" eq 'label') {
        return;    } elsif ("$key" eq 'reference') {
                __docker_complete_images('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--repo', '--tag');
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_253, $out_253);
    my $pid_253 = open3($in_253, $out_253, '>&STDERR', 'compgen', '-S', q{=}, '-W', "before dangling label reference since", '--', "$ENV{cur}");
    close $in_253 or croak 'Close failed: $OS_ERROR';
    my $result_253 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_253> };
    close $out_253 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_253, 0;
    $result_253
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_254, $out_254);
    my $pid_254 = open3($in_254, $out_254, '>&STDERR', 'compgen', '-W', "--all -a --digests --filter -f --format --help --no-trunc --quiet -q", '--', "$ENV{cur}");
    close $in_254 or croak 'Close failed: $OS_ERROR';
    my $result_254 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_254> };
    close $out_254 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_254, 0;
    $result_254
});
    } elsif ("$ENV{cur}" eq '=') {
        return;    } elsif (1) {
                __docker_complete_images('--repo', '--tag');
    }
    return;
}

sub _docker_image_prune {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter') {
                @COMPREPLY = (do {
    my ($in_255, $out_255);
    my $pid_255 = open3($in_255, $out_255, '>&STDERR', 'compgen', '-W', "label label! until", '-S', q{=}, '--', "$ENV{cur}");
    close $in_255 or croak 'Close failed: $OS_ERROR';
    my $result_255 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_255> };
    close $out_255 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_255, 0;
    $result_255
});
                __docker_nospace();
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_256, $out_256);
    my $pid_256 = open3($in_256, $out_256, '>&STDERR', 'compgen', '-W', "--all -a --force -f --filter --help", '--', "$ENV{cur}");
    close $in_256 or croak 'Close failed: $OS_ERROR';
    my $result_256 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_256> };
    close $out_256 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_256, 0;
    $result_256
});
    }
    return;
}

sub _docker_image_pull {
if ("$ENV{prev}" eq '--platform') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                my $options = "--all-tags -a --disable-content-trust=false --help --platform --quiet -q";
                @COMPREPLY = (do {
    my ($in_257, $out_257);
    my $pid_257 = open3($in_257, $out_257, '>&STDERR', 'compgen', '-W', "$options", '--', "$ENV{cur}");
    close $in_257 or croak 'Close failed: $OS_ERROR';
    my $result_257 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_257> };
    close $out_257 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_257, 0;
    $result_257
});
    } elsif (1) {
                my $counter = do {
    my ($in_258, $out_258);
    my $pid_258 = open3($in_258, $out_258, '>&STDERR', '__docker_pos_first_nonflag', '--platform');
    close $in_258 or croak 'Close failed: $OS_ERROR';
    my $result_258 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_258> };
    close $out_258 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_258, 0;
    $result_258
};
        if (($cword == $counter)) {
            my $arg;
            for my $arg (@COMP_WORDS) {
if ("$arg" eq '--all-tags' or "$arg" eq '-a') {
                                        __docker_complete_images('--repo');
                    return;                }
            }
;
            __docker_complete_images('--repo', '--tag');
        }
    }
;
    return;
}

sub _docker_image_push {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_259, $out_259);
    my $pid_259 = open3($in_259, $out_259, '>&STDERR', 'compgen', '-W', "--all-tags -a --disable-content-trust=false --help --quiet -q", '--', "$ENV{cur}");
    close $in_259 or croak 'Close failed: $OS_ERROR';
    my $result_259 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_259> };
    close $out_259 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_259, 0;
    $result_259
});
    } elsif (1) {
                my $counter = do {
    my ($in_260, $out_260);
    my $pid_260 = open3($in_260, $out_260, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_260 or croak 'Close failed: $OS_ERROR';
    my $result_260 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_260> };
    close $out_260 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_260, 0;
    $result_260
};
        if (($cword == $counter)) {
            __docker_complete_images('--repo', '--tag');
        }
    }
;
    return;
}

sub _docker_image_remove {
    $main_exit_code = system('bash', '_docker_image_rm') >> 8;
    return;
}

sub _docker_image_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_261, $out_261);
    my $pid_261 = open3($in_261, $out_261, '>&STDERR', 'compgen', '-W', "--force -f --help --no-prune", '--', "$ENV{cur}");
    close $in_261 or croak 'Close failed: $OS_ERROR';
    my $result_261 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_261> };
    close $out_261 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_261, 0;
    $result_261
});
    } elsif (1) {
                __docker_complete_images('--force-tag', '--id');
    }
;
    return;
}

sub _docker_image_rmi {
    _docker_image_rm();
    return;
}

sub _docker_image_save {
if ("$ENV{prev}" eq '--output' or "$ENV{prev}" eq '-o' or "$ENV{prev}" eq '>') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_262, $out_262);
    my $pid_262 = open3($in_262, $out_262, '>&STDERR', 'compgen', '-W', "--help --output -o", '--', "$ENV{cur}");
    close $in_262 or croak 'Close failed: $OS_ERROR';
    my $result_262 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_262> };
    close $out_262 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_262, 0;
    $result_262
});
    } elsif (1) {
                __docker_complete_images('--repo', '--tag', '--id');
    }
;
    return;
}

sub _docker_image_tag {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_263, $out_263);
    my $pid_263 = open3($in_263, $out_263, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_263 or croak 'Close failed: $OS_ERROR';
    my $result_263 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_263> };
    close $out_263 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_263, 0;
    $result_263
});
    } elsif (1) {
                my $counter = do {
    my ($in_264, $out_264);
    my $pid_264 = open3($in_264, $out_264, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_264 or croak 'Close failed: $OS_ERROR';
    my $result_264 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_264> };
    close $out_264 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_264, 0;
    $result_264
};
        if (($cword == $counter)) {
            __docker_complete_images('--force-tag', '--id');
return;
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                __docker_complete_images('--repo', '--tag');
return;
            }
        }
    }
;
    return;
}

sub _docker_images {
    _docker_image_ls();
    return;
}

sub _docker_import {
    _docker_image_import();
    return;
}

sub _docker_info {
    $main_exit_code = system('bash', '_docker_system_info') >> 8;
    return;
}

sub _docker_inspect {
    my ($file) = @_;
    my $preselected_type;
    my $type;
if ("${1-}" eq "--type") {
        $preselected_type = 'yes';
        $type = "$_[1]";
}
    else {
        $type = do {
    my ($in_265, $out_265);
    my $pid_265 = open3($in_265, $out_265, '>&STDERR', '__docker_value_of_option', '--type');
    close $in_265 or croak 'Close failed: $OS_ERROR';
    my $result_265 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_265> };
    close $out_265 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_265, 0;
    $result_265
};
    }
    my $COMPREPLY;
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    } elsif ("$ENV{prev}" eq '--type') {
        if ("$preselected_type" eq q{}) {
            @COMPREPLY = (do {
    my ($in_266, $out_266);
    my $pid_266 = open3($in_266, $out_266, '>&STDERR', 'compgen', '-W', "container image network node plugin secret service volume", '--', "$ENV{cur}");
    close $in_266 or croak 'Close failed: $OS_ERROR';
    my $result_266 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_266> };
    close $out_266 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_266, 0;
    $result_266
});
return;
        }
    }
;
    my $options;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                my $options = "--format -f --help --size -s";
        if ("$preselected_type" eq q{}) {
            $options = " --type";
        }
                @COMPREPLY = (do {
    my ($in_267, $out_267);
    my $pid_267 = open3($in_267, $out_267, '>&STDERR', 'compgen', '-W', "$options", '--', "$ENV{cur}");
    close $in_267 or croak 'Close failed: $OS_ERROR';
    my $result_267 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_267> };
    close $out_267 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_267, 0;
    $result_267
});
    } elsif (1) {
        if ("$type" eq '') {
                        @COMPREPLY = (do {
    my ($in_276, $out_276);
    my $pid_276 = open3($in_276, $out_276, '>&STDERR', 'compgen', '-W', "
						" . (do {
    my ($in_268, $out_268);
    my $pid_268 = open3($in_268, $out_268, '>&STDERR', '__docker_containers', '--all');
    close $in_268 or croak 'Close failed: $OS_ERROR';
    my $result_268 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_268> };
    close $out_268 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_268, 0;
    $result_268
}) . "
						" . (do {
    my ($in_269, $out_269);
    my $pid_269 = open3($in_269, $out_269, '>&STDERR', '__docker_images', '--force-tag', '--id');
    close $in_269 or croak 'Close failed: $OS_ERROR';
    my $result_269 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_269> };
    close $out_269 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_269, 0;
    $result_269
}) . "
						" . (do {
    my ($in_270, $out_270);
    my $pid_270 = open3($in_270, $out_270, '>&STDERR', '__docker_networks');
    close $in_270 or croak 'Close failed: $OS_ERROR';
    my $result_270 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_270> };
    close $out_270 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_270, 0;
    $result_270
}) . "
						" . (do {
    my ($in_271, $out_271);
    my $pid_271 = open3($in_271, $out_271, '>&STDERR', '__docker_nodes');
    close $in_271 or croak 'Close failed: $OS_ERROR';
    my $result_271 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_271> };
    close $out_271 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_271, 0;
    $result_271
}) . "
						" . (do {
    my ($in_272, $out_272);
    my $pid_272 = open3($in_272, $out_272, '>&STDERR', '__docker_plugins_installed');
    close $in_272 or croak 'Close failed: $OS_ERROR';
    my $result_272 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_272> };
    close $out_272 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_272, 0;
    $result_272
}) . "
						" . (do {
    my ($in_273, $out_273);
    my $pid_273 = open3($in_273, $out_273, '>&STDERR', '__docker_secrets');
    close $in_273 or croak 'Close failed: $OS_ERROR';
    my $result_273 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_273> };
    close $out_273 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_273, 0;
    $result_273
}) . "
						" . (do {
    my ($in_274, $out_274);
    my $pid_274 = open3($in_274, $out_274, '>&STDERR', '__docker_services');
    close $in_274 or croak 'Close failed: $OS_ERROR';
    my $result_274 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_274> };
    close $out_274 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_274, 0;
    $result_274
}) . "
						" . (do {
    my ($in_275, $out_275);
    my $pid_275 = open3($in_275, $out_275, '>&STDERR', '__docker_volumes');
    close $in_275 or croak 'Close failed: $OS_ERROR';
    my $result_275 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_275> };
    close $out_275 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_275, 0;
    $result_275
}) . "
					", '--', "$ENV{cur}");
    close $in_276 or croak 'Close failed: $OS_ERROR';
    my $result_276 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_276> };
    close $out_276 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_276, 0;
    $result_276
});
                        $main_exit_code = system('__ltrim_colon_completions', "$ENV{cur}") >> 8;
        } elsif ("$type" eq 'container') {
                        __docker_complete_containers_all();
        } elsif ("$type" eq 'image') {
                        __docker_complete_images('--force-tag', '--id');
        } elsif ("$type" eq 'network') {
                        __docker_complete_networks();
        } elsif ("$type" eq 'node') {
                        __docker_complete_nodes();
        } elsif ("$type" eq 'plugin') {
                        __docker_complete_plugins_installed();
        } elsif ("$type" eq 'secret') {
                        __docker_complete_secrets();
        } elsif ("$type" eq 'service') {
                        __docker_complete_services();
        } elsif ("$type" eq 'volume') {
                        __docker_complete_volumes();
        }
    }
;
    return;
}

sub _docker_kill {
    _docker_container_kill();
    return;
}

sub _docker_load {
    _docker_image_load();
    return;
}

sub _docker_login {
if ("$ENV{prev}" eq '--password' or "$ENV{prev}" eq '-p' or "$ENV{prev}" eq '--username' or "$ENV{prev}" eq '-u') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_277, $out_277);
    my $pid_277 = open3($in_277, $out_277, '>&STDERR', 'compgen', '-W', "--help --password -p --password-stdin --username -u", '--', "$ENV{cur}");
    close $in_277 or croak 'Close failed: $OS_ERROR';
    my $result_277 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_277> };
    close $out_277 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_277, 0;
    $result_277
});
    }
;
    return;
}

sub _docker_logout {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_278, $out_278);
    my $pid_278 = open3($in_278, $out_278, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_278 or croak 'Close failed: $OS_ERROR';
    my $result_278 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_278> };
    close $out_278 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_278, 0;
    $result_278
});
    }
;
    return;
}

sub _docker_logs {
    _docker_container_logs();
    return;
}

sub _docker_network_connect {
    my $options_with_args = "
		--alias
		--ip
		--ip6
		--link
		--link-local-ip
	";
    my $boolean_options = "
		--help
	";
    my $COMPREPLY;
if ("$ENV{prev}" eq '--link') {
        if ("$ENV{cur}" =~ /^.*:.*$/msx) {
        } elsif (1) {
                        __docker_complete_containers_running();
                        @COMPREPLY = (do {
    my ($in_279, $out_279);
    my $pid_279 = open3($in_279, $out_279, '>&STDERR', 'compgen', '-W', $COMPREPLY[eval { int(*) } // ""], '-S', q{:});
    close $in_279 or croak 'Close failed: $OS_ERROR';
    my $result_279 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_279> };
    close $out_279 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_279, 0;
    $result_279
});
                        __docker_nospace();
        }
        return;    } elsif ("$ENV{prev}" eq '$(__docker_to_extglob "$options_with_args")') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_280, $out_280);
    my $pid_280 = open3($in_280, $out_280, '>&STDERR', 'compgen', '-W', "$boolean_options $options_with_args", '--', "$ENV{cur}");
    close $in_280 or croak 'Close failed: $OS_ERROR';
    my $result_280 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_280> };
    close $out_280 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_280, 0;
    $result_280
});
    } elsif (1) {
                my $counter = do {
    my ($in_282, $out_282);
    my $pid_282 = open3($in_282, $out_282, '>&STDERR', '__docker_pos_first_nonflag', (do {
    my ($in_281, $out_281);
    my $pid_281 = open3($in_281, $out_281, '>&STDERR', '__docker_to_alternatives', "$options_with_args");
    close $in_281 or croak 'Close failed: $OS_ERROR';
    my $result_281 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_281> };
    close $out_281 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_281, 0;
    $result_281
}));
    close $in_282 or croak 'Close failed: $OS_ERROR';
    my $result_282 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_282> };
    close $out_282 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_282, 0;
    $result_282
};
        if (($cword == $counter)) {
            __docker_complete_networks();
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                __docker_complete_containers_all();
            }
        }
    }
    return;
}

sub _docker_network_create {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--aux-address' or "$ENV{prev}" eq '--gateway' or "$ENV{prev}" eq '--ip-range' or "$ENV{prev}" eq '--ipam-opt' or "$ENV{prev}" eq '--ipv6' or "$ENV{prev}" eq '--opt' or "$ENV{prev}" eq '-o' or "$ENV{prev}" eq '--subnet') {
        return;    } elsif ("$ENV{prev}" eq '--config-from') {
                __docker_complete_networks();
        return;    } elsif ("$ENV{prev}" eq '--driver' or "$ENV{prev}" eq '-d') {
                __docker_complete_plugins_bundled('--type', 'Network', '--remove', 'host', '--remove', 'null', '--add', 'macvlan');
        return;    } elsif ("$ENV{prev}" eq '--ipam-driver') {
                @COMPREPLY = (do {
    my ($in_283, $out_283);
    my $pid_283 = open3($in_283, $out_283, '>&STDERR', 'compgen', '-W', "default", '--', "$ENV{cur}");
    close $in_283 or croak 'Close failed: $OS_ERROR';
    my $result_283 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_283> };
    close $out_283 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_283, 0;
    $result_283
});
        return;    } elsif ("$ENV{prev}" eq '--label') {
        return;    } elsif ("$ENV{prev}" eq '--scope') {
                @COMPREPLY = (do {
    my ($in_284, $out_284);
    my $pid_284 = open3($in_284, $out_284, '>&STDERR', 'compgen', '-W', "local swarm", '--', "$ENV{cur}");
    close $in_284 or croak 'Close failed: $OS_ERROR';
    my $result_284 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_284> };
    close $out_284 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_284, 0;
    $result_284
});
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_285, $out_285);
    my $pid_285 = open3($in_285, $out_285, '>&STDERR', 'compgen', '-W', "--attachable --aux-address --config-from --config-only --driver -d --gateway --help --ingress --internal --ip-range --ipam-driver --ipam-opt --ipv6 --label --opt -o --scope --subnet", '--', "$ENV{cur}");
    close $in_285 or croak 'Close failed: $OS_ERROR';
    my $result_285 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_285> };
    close $out_285 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_285, 0;
    $result_285
});
    }
    return;
}

sub _docker_network_disconnect {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_286, $out_286);
    my $pid_286 = open3($in_286, $out_286, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_286 or croak 'Close failed: $OS_ERROR';
    my $result_286 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_286> };
    close $out_286 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_286, 0;
    $result_286
});
    } elsif (1) {
                my $counter = do {
    my ($in_287, $out_287);
    my $pid_287 = open3($in_287, $out_287, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_287 or croak 'Close failed: $OS_ERROR';
    my $result_287 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_287> };
    close $out_287 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_287, 0;
    $result_287
};
        if (($cword == $counter)) {
            __docker_complete_networks();
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                __docker_complete_containers_in_network("$ENV{prev}");
            }
        }
    }
;
    return;
}

sub _docker_network_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_288, $out_288);
    my $pid_288 = open3($in_288, $out_288, '>&STDERR', 'compgen', '-W', "--format -f --help --verbose", '--', "$ENV{cur}");
    close $in_288 or croak 'Close failed: $OS_ERROR';
    my $result_288 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_288> };
    close $out_288 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_288, 0;
    $result_288
});
    } elsif (1) {
                __docker_complete_networks();
    }
;
    return;
}

sub _docker_network_ls {
    my $key = do {
    my ($in_289, $out_289);
    my $pid_289 = open3($in_289, $out_289, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_289 or croak 'Close failed: $OS_ERROR';
    my $result_289 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_289> };
    close $out_289 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_289, 0;
    $result_289
};
    my $COMPREPLY;
if ("$key" eq 'dangling') {
                @COMPREPLY = (do {
    my ($in_290, $out_290);
    my $pid_290 = open3($in_290, $out_290, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_290 or croak 'Close failed: $OS_ERROR';
    my $result_290 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_290> };
    close $out_290 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_290, 0;
    $result_290
});
        return;    } elsif ("$key" eq 'driver') {
                __docker_complete_plugins_bundled('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--type', 'Network', '--add', 'macvlan');
        return;    } elsif ("$key" eq 'id') {
                __docker_complete_networks('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_networks('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    } elsif ("$key" eq 'scope') {
                @COMPREPLY = (do {
    my ($in_291, $out_291);
    my $pid_291 = open3($in_291, $out_291, '>&STDERR', 'compgen', '-W', "global local swarm", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_291 or croak 'Close failed: $OS_ERROR';
    my $result_291 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_291> };
    close $out_291 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_291, 0;
    $result_291
});
        return;    } elsif ("$key" eq 'type') {
                @COMPREPLY = (do {
    my ($in_292, $out_292);
    my $pid_292 = open3($in_292, $out_292, '>&STDERR', 'compgen', '-W', "builtin custom", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_292 or croak 'Close failed: $OS_ERROR';
    my $result_292 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_292> };
    close $out_292 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_292, 0;
    $result_292
});
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_293, $out_293);
    my $pid_293 = open3($in_293, $out_293, '>&STDERR', 'compgen', '-S', q{=}, '-W', "dangling driver id label name scope type", '--', "$ENV{cur}");
    close $in_293 or croak 'Close failed: $OS_ERROR';
    my $result_293 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_293> };
    close $out_293 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_293, 0;
    $result_293
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_294, $out_294);
    my $pid_294 = open3($in_294, $out_294, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --no-trunc --quiet -q", '--', "$ENV{cur}");
    close $in_294 or croak 'Close failed: $OS_ERROR';
    my $result_294 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_294> };
    close $out_294 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_294, 0;
    $result_294
});
    }
    return;
}

sub _docker_network_prune {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter') {
                @COMPREPLY = (do {
    my ($in_295, $out_295);
    my $pid_295 = open3($in_295, $out_295, '>&STDERR', 'compgen', '-W', "label label! until", '-S', q{=}, '--', "$ENV{cur}");
    close $in_295 or croak 'Close failed: $OS_ERROR';
    my $result_295 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_295> };
    close $out_295 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_295, 0;
    $result_295
});
                __docker_nospace();
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_296, $out_296);
    my $pid_296 = open3($in_296, $out_296, '>&STDERR', 'compgen', '-W', "--force -f --filter --help", '--', "$ENV{cur}");
    close $in_296 or croak 'Close failed: $OS_ERROR';
    my $result_296 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_296> };
    close $out_296 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_296, 0;
    $result_296
});
    }
    return;
}

sub _docker_network_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_297, $out_297);
    my $pid_297 = open3($in_297, $out_297, '>&STDERR', 'compgen', '-W', "--force -f --help", '--', "$ENV{cur}");
    close $in_297 or croak 'Close failed: $OS_ERROR';
    my $result_297 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_297> };
    close $out_297 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_297, 0;
    $result_297
});
    } elsif (1) {
                __docker_complete_networks('--filter', 'type', q{=}, 'custom');
    }
;
    return;
}

sub _docker_network {
    my $subcommands = "
		connect
		create
		disconnect
		inspect
		ls
		prune
		rm
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_298, $out_298);
    my $pid_298 = open3($in_298, $out_298, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_298 or croak 'Close failed: $OS_ERROR';
    my $result_298 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_298> };
    close $out_298 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_298, 0;
    $result_298
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_299, $out_299);
    my $pid_299 = open3($in_299, $out_299, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_299 or croak 'Close failed: $OS_ERROR';
    my $result_299 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_299> };
    close $out_299 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_299, 0;
    $result_299
});
    }
;
    return;
}

sub _docker_service {
    my $subcommands = "
		create
		inspect
		logs
		ls
		rm
		rollback
		scale
		ps
		update
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_300, $out_300);
    my $pid_300 = open3($in_300, $out_300, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_300 or croak 'Close failed: $OS_ERROR';
    my $result_300 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_300> };
    close $out_300 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_300, 0;
    $result_300
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_301, $out_301);
    my $pid_301 = open3($in_301, $out_301, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_301 or croak 'Close failed: $OS_ERROR';
    my $result_301 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_301> };
    close $out_301 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_301, 0;
    $result_301
});
    }
;
    return;
}

sub _docker_service_create {
    $main_exit_code = system('bash', '_docker_service_update_and_create') >> 8;
    return;
}

sub _docker_service_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_302, $out_302);
    my $pid_302 = open3($in_302, $out_302, '>&STDERR', 'compgen', '-W', "--format -f --help --pretty", '--', "$ENV{cur}");
    close $in_302 or croak 'Close failed: $OS_ERROR';
    my $result_302 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_302> };
    close $out_302 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_302, 0;
    $result_302
});
    } elsif (1) {
                __docker_complete_services();
    }
;
    return;
}

sub _docker_service_logs {
if ("$ENV{prev}" eq '--since' or "$ENV{prev}" eq '--tail' or "$ENV{prev}" eq '-n') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_303, $out_303);
    my $pid_303 = open3($in_303, $out_303, '>&STDERR', 'compgen', '-W', "--details --follow -f --help --no-resolve --no-task-ids --no-trunc --raw --since --tail -n --timestamps -t", '--', "$ENV{cur}");
    close $in_303 or croak 'Close failed: $OS_ERROR';
    my $result_303 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_303> };
    close $out_303 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_303, 0;
    $result_303
});
    } elsif (1) {
                my $counter = do {
    my ($in_304, $out_304);
    my $pid_304 = open3($in_304, $out_304, '>&STDERR', '__docker_pos_first_nonflag', '--since|--tail|-n');
    close $in_304 or croak 'Close failed: $OS_ERROR';
    my $result_304 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_304> };
    close $out_304 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_304, 0;
    $result_304
};
        if (($cword == $counter)) {
            __docker_complete_services_and_tasks();
        }
    }
;
    return;
}

sub _docker_service_list {
    $main_exit_code = system('bash', '_docker_service_ls') >> 8;
    return;
}

sub _docker_service_ls {
    my $key = do {
    my ($in_305, $out_305);
    my $pid_305 = open3($in_305, $out_305, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_305 or croak 'Close failed: $OS_ERROR';
    my $result_305 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_305> };
    close $out_305 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_305, 0;
    $result_305
};
    my $COMPREPLY;
if ("$key" eq 'id') {
                __docker_complete_services('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'mode') {
                @COMPREPLY = (do {
    my ($in_306, $out_306);
    my $pid_306 = open3($in_306, $out_306, '>&STDERR', 'compgen', '-W', "global global-job replicated replicated-job", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_306 or croak 'Close failed: $OS_ERROR';
    my $result_306 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_306> };
    close $out_306 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_306, 0;
    $result_306
});
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_services('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_307, $out_307);
    my $pid_307 = open3($in_307, $out_307, '>&STDERR', 'compgen', '-W', "id label mode name", '-S', q{=}, '--', "$ENV{cur}");
    close $in_307 or croak 'Close failed: $OS_ERROR';
    my $result_307 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_307> };
    close $out_307 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_307, 0;
    $result_307
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_308, $out_308);
    my $pid_308 = open3($in_308, $out_308, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --quiet -q", '--', "$ENV{cur}");
    close $in_308 or croak 'Close failed: $OS_ERROR';
    my $result_308 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_308> };
    close $out_308 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_308, 0;
    $result_308
});
    }
    return;
}

sub _docker_service_remove {
    $main_exit_code = system('bash', '_docker_service_rm') >> 8;
    return;
}

sub _docker_service_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_309, $out_309);
    my $pid_309 = open3($in_309, $out_309, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_309 or croak 'Close failed: $OS_ERROR';
    my $result_309 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_309> };
    close $out_309 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_309, 0;
    $result_309
});
    } elsif (1) {
                __docker_complete_services();
    }
;
    return;
}

sub _docker_service_rollback {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_310, $out_310);
    my $pid_310 = open3($in_310, $out_310, '>&STDERR', 'compgen', '-W', "--detach -d --help --quit -q", '--', "$ENV{cur}");
    close $in_310 or croak 'Close failed: $OS_ERROR';
    my $result_310 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_310> };
    close $out_310 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_310, 0;
    $result_310
});
    } elsif (1) {
                my $counter = do {
    my ($in_311, $out_311);
    my $pid_311 = open3($in_311, $out_311, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_311 or croak 'Close failed: $OS_ERROR';
    my $result_311 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_311> };
    close $out_311 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_311, 0;
    $result_311
};
        if (($cword == $counter)) {
            __docker_complete_services();
        }
    }
;
    return;
}

sub _docker_service_scale {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_312, $out_312);
    my $pid_312 = open3($in_312, $out_312, '>&STDERR', 'compgen', '-W', "--detach -d --help", '--', "$ENV{cur}");
    close $in_312 or croak 'Close failed: $OS_ERROR';
    my $result_312 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_312> };
    close $out_312 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_312, 0;
    $result_312
});
    } elsif (1) {
                __docker_complete_services();
                __docker_append_to_completions("=");
                __docker_nospace();
    }
;
    return;
}

sub _docker_service_ps {
    my $key = do {
    my ($in_313, $out_313);
    my $pid_313 = open3($in_313, $out_313, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_313 or croak 'Close failed: $OS_ERROR';
    my $result_313 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_313> };
    close $out_313 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_313, 0;
    $result_313
};
    my $COMPREPLY;
if ("$key" eq 'desired-state') {
                @COMPREPLY = (do {
    my ($in_314, $out_314);
    my $pid_314 = open3($in_314, $out_314, '>&STDERR', 'compgen', '-W', "accepted running shutdown", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_314 or croak 'Close failed: $OS_ERROR';
    my $result_314 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_314> };
    close $out_314 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_314, 0;
    $result_314
});
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_services('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    } elsif ("$key" eq 'node') {
                __docker_complete_nodes('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--add', 'self');
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_315, $out_315);
    my $pid_315 = open3($in_315, $out_315, '>&STDERR', 'compgen', '-W', "desired-state id name node", '-S', q{=}, '--', "$ENV{cur}");
    close $in_315 or croak 'Close failed: $OS_ERROR';
    my $result_315 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_315> };
    close $out_315 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_315, 0;
    $result_315
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_316, $out_316);
    my $pid_316 = open3($in_316, $out_316, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --no-resolve --no-trunc --quiet -q", '--', "$ENV{cur}");
    close $in_316 or croak 'Close failed: $OS_ERROR';
    my $result_316 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_316> };
    close $out_316 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_316, 0;
    $result_316
});
    } elsif (1) {
                __docker_complete_services();
    }
    return;
}

sub _docker_service_update {
    $main_exit_code = system('bash', '_docker_service_update_and_create') >> 8;
    return;
}

sub _docker_service_update_and_create {
    my $options_with_args = "
		--cap-add
		--cap-drop
		--endpoint-mode
		--entrypoint
		--health-cmd
		--health-interval
		--health-retries
		--health-start-period
		--health-timeout
		--hostname
		--isolation
		--limit-cpu
		--limit-memory
		--limit-pids
		--log-driver
		--log-opt
		--max-replicas
		--replicas
		--replicas-max-per-node
		--reserve-cpu
		--reserve-memory
		--restart-condition
		--restart-delay
		--restart-max-attempts
		--restart-window
		--rollback-delay
		--rollback-failure-action
		--rollback-max-failure-ratio
		--rollback-monitor
		--rollback-order
		--rollback-parallelism
		--stop-grace-period
		--stop-signal
		--update-delay
		--update-failure-action
		--update-max-failure-ratio
		--update-monitor
		--update-order
		--update-parallelism
		--user -u
		--workdir -w
	";
    if (do {
__docker_server_os_is('windows');
        $CHILD_ERROR == 0
    }) {
                my $credential;
        my $spec;
        $options_with_args = eval { int($options_with_args+"
		--$credential-$spec
	") } // "";
    }
    my $boolean_options = "
		--detach -d
		--help
		--init
		--no-healthcheck
		--no-resolve-image
		--read-only
		--tty -t
		--with-registry-auth
	";
    if (do {
__docker_complete_log_driver_options();
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$subcommand" eq "create") {
        $options_with_args = "$options_with_args
			--config
			--constraint
			--container-label
			--dns
			--dns-option
			--dns-search
			--env -e
			--env-file
			--generic-resource
			--group
			--host
			--label -l
			--mode
			--mount
			--name
			--network
			--placement-pref
			--publish -p
			--secret
			--sysctl
			--ulimit
		";
if ("$ENV{prev}" eq '--env-file') {
                        $main_exit_code = system('bash', '_filedir') >> 8;
            return;        } elsif ("$ENV{prev}" eq '--mode') {
                        @COMPREPLY = (do {
    my ($in_317, $out_317);
    my $pid_317 = open3($in_317, $out_317, '>&STDERR', 'compgen', '-W', "global global-job replicated replicated-job", '--', "$ENV{cur}");
    close $in_317 or croak 'Close failed: $OS_ERROR';
    my $result_317 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_317> };
    close $out_317 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_317, 0;
    $result_317
});
            return;        }
    }
;
if ("$subcommand" eq "update") {
        $options_with_args = "$options_with_args
			--args
			--config-add
			--config-rm
			--constraint-add
			--constraint-rm
			--container-label-add
			--container-label-rm
			--dns-add
			--dns-option-add
			--dns-option-rm
			--dns-rm
			--dns-search-add
			--dns-search-rm
			--env-add
			--env-rm
			--generic-resource-add
			--generic-resource-rm
			--group-add
			--group-rm
			--host-add
			--host-rm
			--image
			--label-add
			--label-rm
			--mount-add
			--mount-rm
			--network-add
			--network-rm
			--placement-pref-add
			--placement-pref-rm
			--publish-add
			--publish-rm
			--rollback
			--secret-add
			--secret-rm
			--sysctl-add
			--sysctl-rm
			--ulimit-add
			--ulimit-rm
		";
        $boolean_options = "$boolean_options
			--force
		";
if ("$ENV{prev}" eq '--env-rm') {
                        @COMPREPLY = (do {
    my ($in_318, $out_318);
    my $pid_318 = open3($in_318, $out_318, '>&STDERR', 'compgen', '-e', '--', "$ENV{cur}");
    close $in_318 or croak 'Close failed: $OS_ERROR';
    my $result_318 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_318> };
    close $out_318 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_318, 0;
    $result_318
});
            return;        } elsif ("$ENV{prev}" eq '--image') {
                        __docker_complete_images('--repo', '--tag', '--id');
            return;        }
    }
    my $strategy = do {
    my ($in_319, $out_319);
    my $pid_319 = open3($in_319, $out_319, '>&STDERR', '__docker_map_key_of_current_option', '--placement-pref|--placement-pref-add|--placement-pref-rm');
    close $in_319 or croak 'Close failed: $OS_ERROR';
    my $result_319 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_319> };
    close $out_319 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_319, 0;
    $result_319
};
if ("$strategy" eq 'spread') {
                @COMPREPLY = (do {
    my ($in_320, $out_320);
    my $pid_320 = open3($in_320, $out_320, '>&STDERR', 'compgen', '-W', "engine.labels node.labels", '-S', q{.}, '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_320 or croak 'Close failed: $OS_ERROR';
    my $result_320 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_320> };
    close $out_320 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_320, 0;
    $result_320
});
                __docker_nospace();
        return;    }
if ("$ENV{prev}" eq '--cap-add') {
                __docker_complete_capabilities_addable();
        return;    } elsif ("$ENV{prev}" eq '--cap-drop') {
                __docker_complete_capabilities_droppable();
        return;    } elsif ("$ENV{prev}" eq '--config' or "$ENV{prev}" eq '--config-add' or "$ENV{prev}" eq '--config-rm') {
                __docker_complete_configs();
        return;    } elsif ("$ENV{prev}" eq '--endpoint-mode') {
                @COMPREPLY = (do {
    my ($in_321, $out_321);
    my $pid_321 = open3($in_321, $out_321, '>&STDERR', 'compgen', '-W', "dnsrr vip", '--', "$ENV{cur}");
    close $in_321 or croak 'Close failed: $OS_ERROR';
    my $result_321 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_321> };
    close $out_321 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_321, 0;
    $result_321
});
        return;    } elsif ("$ENV{prev}" eq '--env' or "$ENV{prev}" eq '-e' or "$ENV{prev}" eq '--env-add') {
                @COMPREPLY = (do {
    my ($in_322, $out_322);
    my $pid_322 = open3($in_322, $out_322, '>&STDERR', 'compgen', '-e', '--', "$ENV{cur}");
    close $in_322 or croak 'Close failed: $OS_ERROR';
    my $result_322 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_322> };
    close $out_322 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_322, 0;
    $result_322
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--group' or "$ENV{prev}" eq '--group-add' or "$ENV{prev}" eq '--group-rm') {
                @COMPREPLY = (do {
    my ($in_323, $out_323);
    my $pid_323 = open3($in_323, $out_323, '>&STDERR', 'compgen', '-g', '--', "$ENV{cur}");
    close $in_323 or croak 'Close failed: $OS_ERROR';
    my $result_323 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_323> };
    close $out_323 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_323, 0;
    $result_323
});
        return;    } elsif ("$ENV{prev}" eq '--host' or "$ENV{prev}" eq '--host-add' or "$ENV{prev}" eq '--host-rm') {
        if ("$ENV{cur}" =~ /^.*:$/msx) {
                        __docker_complete_resolved_hostname();
            return;        }
    } elsif ("$ENV{prev}" eq '--isolation') {
                __docker_complete_isolation();
        return;    } elsif ("$ENV{prev}" eq '--log-driver') {
                __docker_complete_log_drivers();
        return;    } elsif ("$ENV{prev}" eq '--log-opt') {
                __docker_complete_log_options();
        return;    } elsif ("$ENV{prev}" eq '--network' or "$ENV{prev}" eq '--network-add' or "$ENV{prev}" eq '--network-rm') {
                __docker_complete_networks();
        return;    } elsif ("$ENV{prev}" eq '--placement-pref' or "$ENV{prev}" eq '--placement-pref-add' or "$ENV{prev}" eq '--placement-pref-rm') {
                @COMPREPLY = (do {
    my ($in_324, $out_324);
    my $pid_324 = open3($in_324, $out_324, '>&STDERR', 'compgen', '-W', "spread", '-S', q{=}, '--', "$ENV{cur}");
    close $in_324 or croak 'Close failed: $OS_ERROR';
    my $result_324 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_324> };
    close $out_324 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_324, 0;
    $result_324
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--restart-condition') {
                @COMPREPLY = (do {
    my ($in_325, $out_325);
    my $pid_325 = open3($in_325, $out_325, '>&STDERR', 'compgen', '-W', "any none on-failure", '--', "$ENV{cur}");
    close $in_325 or croak 'Close failed: $OS_ERROR';
    my $result_325 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_325> };
    close $out_325 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_325, 0;
    $result_325
});
        return;    } elsif ("$ENV{prev}" eq '--rollback-failure-action') {
                @COMPREPLY = (do {
    my ($in_326, $out_326);
    my $pid_326 = open3($in_326, $out_326, '>&STDERR', 'compgen', '-W', "continue pause", '--', "$ENV{cur}");
    close $in_326 or croak 'Close failed: $OS_ERROR';
    my $result_326 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_326> };
    close $out_326 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_326, 0;
    $result_326
});
        return;    } elsif ("$ENV{prev}" eq '--secret' or "$ENV{prev}" eq '--secret-add' or "$ENV{prev}" eq '--secret-rm') {
                __docker_complete_secrets();
        return;    } elsif ("$ENV{prev}" eq '--stop-signal') {
                __docker_complete_signals();
        return;    } elsif ("$ENV{prev}" eq '--update-failure-action') {
                @COMPREPLY = (do {
    my ($in_327, $out_327);
    my $pid_327 = open3($in_327, $out_327, '>&STDERR', 'compgen', '-W', "continue pause rollback", '--', "$ENV{cur}");
    close $in_327 or croak 'Close failed: $OS_ERROR';
    my $result_327 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_327> };
    close $out_327 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_327, 0;
    $result_327
});
        return;    } elsif ("$ENV{prev}" eq '--ulimit' or "$ENV{prev}" eq '--ulimit-add') {
                __docker_complete_ulimits();
        return;    } elsif ("$ENV{prev}" eq '--ulimit-rm') {
                __docker_complete_ulimits('--rm');
        return;    } elsif ("$ENV{prev}" eq '--update-order' or "$ENV{prev}" eq '--rollback-order') {
                @COMPREPLY = (do {
    my ($in_328, $out_328);
    my $pid_328 = open3($in_328, $out_328, '>&STDERR', 'compgen', '-W', "start-first stop-first", '--', "$ENV{cur}");
    close $in_328 or croak 'Close failed: $OS_ERROR';
    my $result_328 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_328> };
    close $out_328 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_328, 0;
    $result_328
});
        return;    } elsif ("$ENV{prev}" eq '--user' or "$ENV{prev}" eq '-u') {
                __docker_complete_user_group();
        return;    } elsif ("$ENV{prev}" eq '$(__docker_to_extglob "$options_with_args")') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_329, $out_329);
    my $pid_329 = open3($in_329, $out_329, '>&STDERR', 'compgen', '-W', "$boolean_options $options_with_args", '--', "$ENV{cur}");
    close $in_329 or croak 'Close failed: $OS_ERROR';
    my $result_329 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_329> };
    close $out_329 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_329, 0;
    $result_329
});
    } elsif (1) {
                my $counter = do {
    my ($in_331, $out_331);
    my $pid_331 = open3($in_331, $out_331, '>&STDERR', '__docker_pos_first_nonflag', (do {
    my ($in_330, $out_330);
    my $pid_330 = open3($in_330, $out_330, '>&STDERR', '__docker_to_alternatives', "$options_with_args");
    close $in_330 or croak 'Close failed: $OS_ERROR';
    my $result_330 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_330> };
    close $out_330 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_330, 0;
    $result_330
}));
    close $in_331 or croak 'Close failed: $OS_ERROR';
    my $result_331 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_331> };
    close $out_331 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_331, 0;
    $result_331
};
        if ("$subcommand" eq "update") {
if (($cword == $counter)) {
                __docker_complete_services();
            }
}
        else {
if (($cword == $counter)) {
                __docker_complete_images('--repo', '--tag', '--id');
            }
        }
    }
    return;
}

sub _docker_swarm {
    my $subcommands = "
		ca
		init
		join
		join-token
		leave
		unlock
		unlock-key
		update
	";
    if (do {
__docker_subcommands("$subcommands");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_332, $out_332);
    my $pid_332 = open3($in_332, $out_332, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_332 or croak 'Close failed: $OS_ERROR';
    my $result_332 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_332> };
    close $out_332 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_332, 0;
    $result_332
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_333, $out_333);
    my $pid_333 = open3($in_333, $out_333, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_333 or croak 'Close failed: $OS_ERROR';
    my $result_333 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_333> };
    close $out_333 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_333, 0;
    $result_333
});
    }
;
    return;
}

sub _docker_swarm_ca {
if ("$ENV{prev}" eq '--ca-cert' or "$ENV{prev}" eq '--ca-key') {
                $main_exit_code = system('bash', '_filedir') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--cert-expiry' or "$ENV{prev}" eq '--external-ca') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_334, $out_334);
    my $pid_334 = open3($in_334, $out_334, '>&STDERR', 'compgen', '-W', "--ca-cert --ca-key --cert-expiry --detach -d --external-ca --help --quiet -q --rotate", '--', "$ENV{cur}");
    close $in_334 or croak 'Close failed: $OS_ERROR';
    my $result_334 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_334> };
    close $out_334 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_334, 0;
    $result_334
});
    }
;
    return;
}

sub _docker_swarm_init {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--advertise-addr') {
        if ($cur =~ /[*]:/msx) {
            @COMPREPLY = (do {
    my ($in_335, $out_335);
    my $pid_335 = open3($in_335, $out_335, '>&STDERR', 'compgen', '-W', "2377", '--', (($ENV{cur} // q{}) =~ s/^.*://sr =~ s/^.*://sr));
    close $in_335 or croak 'Close failed: $OS_ERROR';
    my $result_335 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_335> };
    close $out_335 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_335, 0;
    $result_335
});
}
        else {
            __docker_complete_local_interfaces();
            __docker_nospace();
        }
        return;    } elsif ("$ENV{prev}" eq '--availability') {
                @COMPREPLY = (do {
    my ($in_336, $out_336);
    my $pid_336 = open3($in_336, $out_336, '>&STDERR', 'compgen', '-W', "active drain pause", '--', "$ENV{cur}");
    close $in_336 or croak 'Close failed: $OS_ERROR';
    my $result_336 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_336> };
    close $out_336 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_336, 0;
    $result_336
});
        return;    } elsif ("$ENV{prev}" eq '--cert-expiry' or "$ENV{prev}" eq '--data-path-port' or "$ENV{prev}" eq '--default-addr-pool' or "$ENV{prev}" eq '--default-addr-pool-mask-length' or "$ENV{prev}" eq '--dispatcher-heartbeat' or "$ENV{prev}" eq '--external-ca' or "$ENV{prev}" eq '--max-snapshots' or "$ENV{prev}" eq '--snapshot-interval' or "$ENV{prev}" eq '--task-history-limit') {
        return;    } elsif ("$ENV{prev}" eq '--data-path-addr') {
                __docker_complete_local_interfaces();
        return;    } elsif ("$ENV{prev}" eq '--listen-addr') {
        if ($cur =~ /[*]:/msx) {
            @COMPREPLY = (do {
    my ($in_337, $out_337);
    my $pid_337 = open3($in_337, $out_337, '>&STDERR', 'compgen', '-W', "2377", '--', (($ENV{cur} // q{}) =~ s/^.*://sr =~ s/^.*://sr));
    close $in_337 or croak 'Close failed: $OS_ERROR';
    my $result_337 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_337> };
    close $out_337 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_337, 0;
    $result_337
});
}
        else {
            __docker_complete_local_interfaces('--add', '0.0.0.0');
            __docker_nospace();
        }
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_338, $out_338);
    my $pid_338 = open3($in_338, $out_338, '>&STDERR', 'compgen', '-W', "--advertise-addr --autolock --availability --cert-expiry --data-path-addr --data-path-port --default-addr-pool --default-addr-pool-mask-length --dispatcher-heartbeat --external-ca --force-new-cluster --help --listen-addr --max-snapshots --snapshot-interval --task-history-limit ", '--', "$ENV{cur}");
    close $in_338 or croak 'Close failed: $OS_ERROR';
    my $result_338 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_338> };
    close $out_338 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_338, 0;
    $result_338
});
    }
    return;
}

sub _docker_swarm_join {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--advertise-addr') {
        if ($cur =~ /[*]:/msx) {
            @COMPREPLY = (do {
    my ($in_339, $out_339);
    my $pid_339 = open3($in_339, $out_339, '>&STDERR', 'compgen', '-W', "2377", '--', (($ENV{cur} // q{}) =~ s/^.*://sr =~ s/^.*://sr));
    close $in_339 or croak 'Close failed: $OS_ERROR';
    my $result_339 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_339> };
    close $out_339 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_339, 0;
    $result_339
});
}
        else {
            __docker_complete_local_interfaces();
            __docker_nospace();
        }
        return;    } elsif ("$ENV{prev}" eq '--availability') {
                @COMPREPLY = (do {
    my ($in_340, $out_340);
    my $pid_340 = open3($in_340, $out_340, '>&STDERR', 'compgen', '-W', "active drain pause", '--', "$ENV{cur}");
    close $in_340 or croak 'Close failed: $OS_ERROR';
    my $result_340 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_340> };
    close $out_340 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_340, 0;
    $result_340
});
        return;    } elsif ("$ENV{prev}" eq '--data-path-addr') {
                __docker_complete_local_interfaces();
        return;    } elsif ("$ENV{prev}" eq '--listen-addr') {
        if ($cur =~ /[*]:/msx) {
            @COMPREPLY = (do {
    my ($in_341, $out_341);
    my $pid_341 = open3($in_341, $out_341, '>&STDERR', 'compgen', '-W', "2377", '--', (($ENV{cur} // q{}) =~ s/^.*://sr =~ s/^.*://sr));
    close $in_341 or croak 'Close failed: $OS_ERROR';
    my $result_341 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_341> };
    close $out_341 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_341, 0;
    $result_341
});
}
        else {
            __docker_complete_local_interfaces('--add', '0.0.0.0');
            __docker_nospace();
        }
        return;    } elsif ("$ENV{prev}" eq '--token') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_342, $out_342);
    my $pid_342 = open3($in_342, $out_342, '>&STDERR', 'compgen', '-W', "--advertise-addr --availability --data-path-addr --help --listen-addr --token", '--', "$ENV{cur}");
    close $in_342 or croak 'Close failed: $OS_ERROR';
    my $result_342 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_342> };
    close $out_342 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_342, 0;
    $result_342
});
    } elsif ("$ENV{cur}" =~ /^.*:$/msx) {
                @COMPREPLY = (do {
    my ($in_343, $out_343);
    my $pid_343 = open3($in_343, $out_343, '>&STDERR', 'compgen', '-W', "2377", '--', (($ENV{cur} // q{}) =~ s/^.*://sr =~ s/^.*://sr));
    close $in_343 or croak 'Close failed: $OS_ERROR';
    my $result_343 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_343> };
    close $out_343 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_343, 0;
    $result_343
});
    }
    return;
}

sub _docker_swarm_join_token {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_344, $out_344);
    my $pid_344 = open3($in_344, $out_344, '>&STDERR', 'compgen', '-W', "--help --quiet -q --rotate", '--', "$ENV{cur}");
    close $in_344 or croak 'Close failed: $OS_ERROR';
    my $result_344 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_344> };
    close $out_344 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_344, 0;
    $result_344
});
    } elsif (1) {
                my $counter = do {
    my ($in_345, $out_345);
    my $pid_345 = open3($in_345, $out_345, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_345 or croak 'Close failed: $OS_ERROR';
    my $result_345 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_345> };
    close $out_345 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_345, 0;
    $result_345
};
        if (($cword == $counter)) {
            @COMPREPLY = (do {
    my ($in_346, $out_346);
    my $pid_346 = open3($in_346, $out_346, '>&STDERR', 'compgen', '-W', "manager worker", '--', "$ENV{cur}");
    close $in_346 or croak 'Close failed: $OS_ERROR';
    my $result_346 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_346> };
    close $out_346 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_346, 0;
    $result_346
});
        }
    }
;
    return;
}

sub _docker_swarm_leave {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_347, $out_347);
    my $pid_347 = open3($in_347, $out_347, '>&STDERR', 'compgen', '-W', "--force -f --help", '--', "$ENV{cur}");
    close $in_347 or croak 'Close failed: $OS_ERROR';
    my $result_347 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_347> };
    close $out_347 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_347, 0;
    $result_347
});
    }
;
    return;
}

sub _docker_swarm_unlock {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_348, $out_348);
    my $pid_348 = open3($in_348, $out_348, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_348 or croak 'Close failed: $OS_ERROR';
    my $result_348 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_348> };
    close $out_348 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_348, 0;
    $result_348
});
    }
;
    return;
}

sub _docker_swarm_unlock_key {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_349, $out_349);
    my $pid_349 = open3($in_349, $out_349, '>&STDERR', 'compgen', '-W', "--help --quiet -q --rotate", '--', "$ENV{cur}");
    close $in_349 or croak 'Close failed: $OS_ERROR';
    my $result_349 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_349> };
    close $out_349 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_349, 0;
    $result_349
});
    }
;
    return;
}

sub _docker_swarm_update {
if ("$ENV{prev}" eq '--cert-expiry' or "$ENV{prev}" eq '--dispatcher-heartbeat' or "$ENV{prev}" eq '--external-ca' or "$ENV{prev}" eq '--max-snapshots' or "$ENV{prev}" eq '--snapshot-interval' or "$ENV{prev}" eq '--task-history-limit') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_350, $out_350);
    my $pid_350 = open3($in_350, $out_350, '>&STDERR', 'compgen', '-W', "--autolock --cert-expiry --dispatcher-heartbeat --external-ca --help --max-snapshots --snapshot-interval --task-history-limit", '--', "$ENV{cur}");
    close $in_350 or croak 'Close failed: $OS_ERROR';
    my $result_350 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_350> };
    close $out_350 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_350, 0;
    $result_350
});
    }
;
    return;
}

sub _docker_manifest {
    my $subcommands = "
		annotate
		create
		inspect
		push
		rm
	";
    if (do {
__docker_subcommands("$subcommands");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_351, $out_351);
    my $pid_351 = open3($in_351, $out_351, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_351 or croak 'Close failed: $OS_ERROR';
    my $result_351 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_351> };
    close $out_351 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_351, 0;
    $result_351
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_352, $out_352);
    my $pid_352 = open3($in_352, $out_352, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_352 or croak 'Close failed: $OS_ERROR';
    my $result_352 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_352> };
    close $out_352 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_352, 0;
    $result_352
});
    }
;
    return;
}

sub _docker_manifest_annotate {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--arch') {
                @COMPREPLY = (do {
    my ($in_353, $out_353);
    my $pid_353 = open3($in_353, $out_353, '>&STDERR', 'compgen', '-W', "
				386
				amd64
				arm
				arm64
				mips64
				mips64le
				ppc64le
				riscv64
				s390x", '--', "$ENV{cur}");
    close $in_353 or croak 'Close failed: $OS_ERROR';
    my $result_353 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_353> };
    close $out_353 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_353, 0;
    $result_353
});
        return;    } elsif ("$ENV{prev}" eq '--os') {
                @COMPREPLY = (do {
    my ($in_354, $out_354);
    my $pid_354 = open3($in_354, $out_354, '>&STDERR', 'compgen', '-W', "
				darwin
				dragonfly
				freebsd
				linux
				netbsd
				openbsd
				plan9
				solaris
				windows", '--', "$ENV{cur}");
    close $in_354 or croak 'Close failed: $OS_ERROR';
    my $result_354 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_354> };
    close $out_354 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_354, 0;
    $result_354
});
        return;    } elsif ("$ENV{prev}" eq '--os-features' or "$ENV{prev}" eq '--variant') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_355, $out_355);
    my $pid_355 = open3($in_355, $out_355, '>&STDERR', 'compgen', '-W', "--arch --help --os --os-features --variant", '--', "$ENV{cur}");
    close $in_355 or croak 'Close failed: $OS_ERROR';
    my $result_355 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_355> };
    close $out_355 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_355, 0;
    $result_355
});
    } elsif (1) {
                my $counter = do {
    my ($in_356, $out_356);
    my $pid_356 = open3($in_356, $out_356, '>&STDERR', '__docker_pos_first_nonflag', "--arch|--os|--os-features|--variant");
    close $in_356 or croak 'Close failed: $OS_ERROR';
    my $result_356 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_356> };
    close $out_356 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_356, 0;
    $result_356
};
        if ((($cword == $counter) || ($cword == (eval { int($counter + 1) } // "")))) {
            __docker_complete_images('--force-tag', '--id');
        }
    }
    return;
}

sub _docker_manifest_create {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_357, $out_357);
    my $pid_357 = open3($in_357, $out_357, '>&STDERR', 'compgen', '-W', "--amend -a --help --insecure", '--', "$ENV{cur}");
    close $in_357 or croak 'Close failed: $OS_ERROR';
    my $result_357 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_357> };
    close $out_357 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_357, 0;
    $result_357
});
    } elsif (1) {
                __docker_complete_images('--force-tag', '--id');
    }
;
    return;
}

sub _docker_manifest_inspect {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_358, $out_358);
    my $pid_358 = open3($in_358, $out_358, '>&STDERR', 'compgen', '-W', "--help --insecure --verbose -v", '--', "$ENV{cur}");
    close $in_358 or croak 'Close failed: $OS_ERROR';
    my $result_358 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_358> };
    close $out_358 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_358, 0;
    $result_358
});
    } elsif (1) {
                my $counter = do {
    my ($in_359, $out_359);
    my $pid_359 = open3($in_359, $out_359, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_359 or croak 'Close failed: $OS_ERROR';
    my $result_359 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_359> };
    close $out_359 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_359, 0;
    $result_359
};
        if ((($cword == $counter) || ($cword == (eval { int($counter + 1) } // "")))) {
            __docker_complete_images('--force-tag', '--id');
        }
    }
;
    return;
}

sub _docker_manifest_push {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_360, $out_360);
    my $pid_360 = open3($in_360, $out_360, '>&STDERR', 'compgen', '-W', "--help --insecure --purge -p", '--', "$ENV{cur}");
    close $in_360 or croak 'Close failed: $OS_ERROR';
    my $result_360 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_360> };
    close $out_360 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_360, 0;
    $result_360
});
    } elsif (1) {
                my $counter = do {
    my ($in_361, $out_361);
    my $pid_361 = open3($in_361, $out_361, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_361 or croak 'Close failed: $OS_ERROR';
    my $result_361 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_361> };
    close $out_361 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_361, 0;
    $result_361
};
        if (($cword == $counter)) {
            __docker_complete_images('--force-tag', '--id');
        }
    }
;
    return;
}

sub _docker_manifest_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_362, $out_362);
    my $pid_362 = open3($in_362, $out_362, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_362 or croak 'Close failed: $OS_ERROR';
    my $result_362 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_362> };
    close $out_362 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_362, 0;
    $result_362
});
    } elsif (1) {
                __docker_complete_images('--force-tag', '--id');
    }
;
    return;
}

sub _docker_node {
    my $subcommands = "
		demote
		inspect
		ls
		promote
		rm
		ps
		update
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_363, $out_363);
    my $pid_363 = open3($in_363, $out_363, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_363 or croak 'Close failed: $OS_ERROR';
    my $result_363 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_363> };
    close $out_363 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_363, 0;
    $result_363
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_364, $out_364);
    my $pid_364 = open3($in_364, $out_364, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_364 or croak 'Close failed: $OS_ERROR';
    my $result_364 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_364> };
    close $out_364 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_364, 0;
    $result_364
});
    }
;
    return;
}

sub _docker_node_demote {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_365, $out_365);
    my $pid_365 = open3($in_365, $out_365, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_365 or croak 'Close failed: $OS_ERROR';
    my $result_365 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_365> };
    close $out_365 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_365, 0;
    $result_365
});
    } elsif (1) {
                __docker_complete_nodes('--filter', 'role', q{=}, 'manager');
    }
;
    return;
}

sub _docker_node_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_366, $out_366);
    my $pid_366 = open3($in_366, $out_366, '>&STDERR', 'compgen', '-W', "--format -f --help --pretty", '--', "$ENV{cur}");
    close $in_366 or croak 'Close failed: $OS_ERROR';
    my $result_366 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_366> };
    close $out_366 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_366, 0;
    $result_366
});
    } elsif (1) {
                __docker_complete_nodes('--add', 'self');
    }
;
    return;
}

sub _docker_node_list {
    $main_exit_code = system('bash', '_docker_node_ls') >> 8;
    return;
}

sub _docker_node_ls {
    my $key = do {
    my ($in_367, $out_367);
    my $pid_367 = open3($in_367, $out_367, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_367 or croak 'Close failed: $OS_ERROR';
    my $result_367 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_367> };
    close $out_367 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_367, 0;
    $result_367
};
    my $COMPREPLY;
if ("$key" eq 'id') {
                __docker_complete_nodes('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'label' or "$key" eq 'node.label') {
        return;    } elsif ("$key" eq 'membership') {
                @COMPREPLY = (do {
    my ($in_368, $out_368);
    my $pid_368 = open3($in_368, $out_368, '>&STDERR', 'compgen', '-W', "accepted pending", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_368 or croak 'Close failed: $OS_ERROR';
    my $result_368 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_368> };
    close $out_368 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_368, 0;
    $result_368
});
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_nodes('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    } elsif ("$key" eq 'role') {
                @COMPREPLY = (do {
    my ($in_369, $out_369);
    my $pid_369 = open3($in_369, $out_369, '>&STDERR', 'compgen', '-W', "manager worker", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_369 or croak 'Close failed: $OS_ERROR';
    my $result_369 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_369> };
    close $out_369 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_369, 0;
    $result_369
});
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_370, $out_370);
    my $pid_370 = open3($in_370, $out_370, '>&STDERR', 'compgen', '-W', "id label membership name node.label role", '-S', q{=}, '--', "$ENV{cur}");
    close $in_370 or croak 'Close failed: $OS_ERROR';
    my $result_370 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_370> };
    close $out_370 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_370, 0;
    $result_370
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_371, $out_371);
    my $pid_371 = open3($in_371, $out_371, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --quiet -q", '--', "$ENV{cur}");
    close $in_371 or croak 'Close failed: $OS_ERROR';
    my $result_371 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_371> };
    close $out_371 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_371, 0;
    $result_371
});
    }
    return;
}

sub _docker_node_promote {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_372, $out_372);
    my $pid_372 = open3($in_372, $out_372, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_372 or croak 'Close failed: $OS_ERROR';
    my $result_372 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_372> };
    close $out_372 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_372, 0;
    $result_372
});
    } elsif (1) {
                __docker_complete_nodes('--filter', 'role', q{=}, 'worker');
    }
;
    return;
}

sub _docker_node_remove {
    $main_exit_code = system('bash', '_docker_node_rm') >> 8;
    return;
}

sub _docker_node_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_373, $out_373);
    my $pid_373 = open3($in_373, $out_373, '>&STDERR', 'compgen', '-W', "--force -f --help", '--', "$ENV{cur}");
    close $in_373 or croak 'Close failed: $OS_ERROR';
    my $result_373 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_373> };
    close $out_373 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_373, 0;
    $result_373
});
    } elsif (1) {
                __docker_complete_nodes();
    }
;
    return;
}

sub _docker_node_ps {
    my $key = do {
    my ($in_374, $out_374);
    my $pid_374 = open3($in_374, $out_374, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_374 or croak 'Close failed: $OS_ERROR';
    my $result_374 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_374> };
    close $out_374 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_374, 0;
    $result_374
};
    my $COMPREPLY;
if ("$key" eq 'desired-state') {
                @COMPREPLY = (do {
    my ($in_375, $out_375);
    my $pid_375 = open3($in_375, $out_375, '>&STDERR', 'compgen', '-W', "accepted running shutdown", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_375 or croak 'Close failed: $OS_ERROR';
    my $result_375 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_375> };
    close $out_375 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_375, 0;
    $result_375
});
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_services('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_376, $out_376);
    my $pid_376 = open3($in_376, $out_376, '>&STDERR', 'compgen', '-W', "desired-state id label name", '-S', q{=}, '--', "$ENV{cur}");
    close $in_376 or croak 'Close failed: $OS_ERROR';
    my $result_376 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_376> };
    close $out_376 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_376, 0;
    $result_376
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_377, $out_377);
    my $pid_377 = open3($in_377, $out_377, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --no-resolve --no-trunc --quiet -q", '--', "$ENV{cur}");
    close $in_377 or croak 'Close failed: $OS_ERROR';
    my $result_377 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_377> };
    close $out_377 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_377, 0;
    $result_377
});
    } elsif (1) {
                __docker_complete_nodes('--add', 'self');
    }
    return;
}

sub _docker_node_update {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--availability') {
                @COMPREPLY = (do {
    my ($in_378, $out_378);
    my $pid_378 = open3($in_378, $out_378, '>&STDERR', 'compgen', '-W', "active drain pause", '--', "$ENV{cur}");
    close $in_378 or croak 'Close failed: $OS_ERROR';
    my $result_378 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_378> };
    close $out_378 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_378, 0;
    $result_378
});
        return;    } elsif ("$ENV{prev}" eq '--role') {
                @COMPREPLY = (do {
    my ($in_379, $out_379);
    my $pid_379 = open3($in_379, $out_379, '>&STDERR', 'compgen', '-W', "manager worker", '--', "$ENV{cur}");
    close $in_379 or croak 'Close failed: $OS_ERROR';
    my $result_379 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_379> };
    close $out_379 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_379, 0;
    $result_379
});
        return;    } elsif ("$ENV{prev}" eq '--label-add' or "$ENV{prev}" eq '--label-rm') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_380, $out_380);
    my $pid_380 = open3($in_380, $out_380, '>&STDERR', 'compgen', '-W', "--availability --help --label-add --label-rm --role", '--', "$ENV{cur}");
    close $in_380 or croak 'Close failed: $OS_ERROR';
    my $result_380 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_380> };
    close $out_380 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_380, 0;
    $result_380
});
    } elsif (1) {
                my $counter = do {
    my ($in_381, $out_381);
    my $pid_381 = open3($in_381, $out_381, '>&STDERR', '__docker_pos_first_nonflag', '--availability|--label-add|--label-rm|--role');
    close $in_381 or croak 'Close failed: $OS_ERROR';
    my $result_381 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_381> };
    close $out_381 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_381, 0;
    $result_381
};
        if (($cword == $counter)) {
            __docker_complete_nodes();
        }
    }
    return;
}

sub _docker_pause {
    _docker_container_pause();
    return;
}

sub _docker_plugin {
    my $subcommands = "
		create
		disable
		enable
		inspect
		install
		ls
		push
		rm
		set
		upgrade
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_382, $out_382);
    my $pid_382 = open3($in_382, $out_382, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_382 or croak 'Close failed: $OS_ERROR';
    my $result_382 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_382> };
    close $out_382 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_382, 0;
    $result_382
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_383, $out_383);
    my $pid_383 = open3($in_383, $out_383, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_383 or croak 'Close failed: $OS_ERROR';
    my $result_383 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_383> };
    close $out_383 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_383, 0;
    $result_383
});
    }
;
    return;
}

sub _docker_plugin_create {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_384, $out_384);
    my $pid_384 = open3($in_384, $out_384, '>&STDERR', 'compgen', '-W', "--compress --help", '--', "$ENV{cur}");
    close $in_384 or croak 'Close failed: $OS_ERROR';
    my $result_384 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_384> };
    close $out_384 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_384, 0;
    $result_384
});
    } elsif (1) {
                my $counter = do {
    my ($in_385, $out_385);
    my $pid_385 = open3($in_385, $out_385, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_385 or croak 'Close failed: $OS_ERROR';
    my $result_385 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_385> };
    close $out_385 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_385, 0;
    $result_385
};
        if (($cword == $counter)) {
return;
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                $main_exit_code = system('_filedir', '-d') >> 8;
            }
        }
    }
;
    return;
}

sub _docker_plugin_disable {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_386, $out_386);
    my $pid_386 = open3($in_386, $out_386, '>&STDERR', 'compgen', '-W', "--force -f --help", '--', "$ENV{cur}");
    close $in_386 or croak 'Close failed: $OS_ERROR';
    my $result_386 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_386> };
    close $out_386 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_386, 0;
    $result_386
});
    } elsif (1) {
                my $counter = do {
    my ($in_387, $out_387);
    my $pid_387 = open3($in_387, $out_387, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_387 or croak 'Close failed: $OS_ERROR';
    my $result_387 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_387> };
    close $out_387 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_387, 0;
    $result_387
};
        if (($cword == $counter)) {
            __docker_complete_plugins_installed('--filter', 'enabled', q{=}, 'true');
        }
    }
;
    return;
}

sub _docker_plugin_enable {
if ("$ENV{prev}" eq '--timeout') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_388, $out_388);
    my $pid_388 = open3($in_388, $out_388, '>&STDERR', 'compgen', '-W', "--help --timeout", '--', "$ENV{cur}");
    close $in_388 or croak 'Close failed: $OS_ERROR';
    my $result_388 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_388> };
    close $out_388 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_388, 0;
    $result_388
});
    } elsif (1) {
                my $counter = do {
    my ($in_389, $out_389);
    my $pid_389 = open3($in_389, $out_389, '>&STDERR', '__docker_pos_first_nonflag', '--timeout');
    close $in_389 or croak 'Close failed: $OS_ERROR';
    my $result_389 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_389> };
    close $out_389 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_389, 0;
    $result_389
};
        if (($cword == $counter)) {
            __docker_complete_plugins_installed('--filter', 'enabled', q{=}, 'false');
        }
    }
;
    return;
}

sub _docker_plugin_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq 'f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_390, $out_390);
    my $pid_390 = open3($in_390, $out_390, '>&STDERR', 'compgen', '-W', "--format -f --help", '--', "$ENV{cur}");
    close $in_390 or croak 'Close failed: $OS_ERROR';
    my $result_390 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_390> };
    close $out_390 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_390, 0;
    $result_390
});
    } elsif (1) {
                __docker_complete_plugins_installed();
    }
;
    return;
}

sub _docker_plugin_install {
if ("$ENV{prev}" eq '--alias') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_391, $out_391);
    my $pid_391 = open3($in_391, $out_391, '>&STDERR', 'compgen', '-W', "--alias --disable --disable-content-trust=false --grant-all-permissions --help", '--', "$ENV{cur}");
    close $in_391 or croak 'Close failed: $OS_ERROR';
    my $result_391 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_391> };
    close $out_391 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_391, 0;
    $result_391
});
    }
;
    return;
}

sub _docker_plugin_list {
    $main_exit_code = system('bash', '_docker_plugin_ls') >> 8;
    return;
}

sub _docker_plugin_ls {
    my $key = do {
    my ($in_392, $out_392);
    my $pid_392 = open3($in_392, $out_392, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_392 or croak 'Close failed: $OS_ERROR';
    my $result_392 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_392> };
    close $out_392 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_392, 0;
    $result_392
};
    my $COMPREPLY;
if ("$key" eq 'capability') {
                @COMPREPLY = (do {
    my ($in_393, $out_393);
    my $pid_393 = open3($in_393, $out_393, '>&STDERR', 'compgen', '-W', "authz ipamdriver logdriver metricscollector networkdriver volumedriver", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_393 or croak 'Close failed: $OS_ERROR';
    my $result_393 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_393> };
    close $out_393 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_393, 0;
    $result_393
});
        return;    } elsif ("$key" eq 'enabled') {
                @COMPREPLY = (do {
    my ($in_394, $out_394);
    my $pid_394 = open3($in_394, $out_394, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_394 or croak 'Close failed: $OS_ERROR';
    my $result_394 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_394> };
    close $out_394 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_394, 0;
    $result_394
});
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_395, $out_395);
    my $pid_395 = open3($in_395, $out_395, '>&STDERR', 'compgen', '-S', q{=}, '-W', "capability enabled", '--', "$ENV{cur}");
    close $in_395 or croak 'Close failed: $OS_ERROR';
    my $result_395 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_395> };
    close $out_395 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_395, 0;
    $result_395
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_396, $out_396);
    my $pid_396 = open3($in_396, $out_396, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --no-trunc --quiet -q", '--', "$ENV{cur}");
    close $in_396 or croak 'Close failed: $OS_ERROR';
    my $result_396 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_396> };
    close $out_396 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_396, 0;
    $result_396
});
    }
    return;
}

sub _docker_plugin_push {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_397, $out_397);
    my $pid_397 = open3($in_397, $out_397, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_397 or croak 'Close failed: $OS_ERROR';
    my $result_397 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_397> };
    close $out_397 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_397, 0;
    $result_397
});
    } elsif (1) {
                my $counter = do {
    my ($in_398, $out_398);
    my $pid_398 = open3($in_398, $out_398, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_398 or croak 'Close failed: $OS_ERROR';
    my $result_398 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_398> };
    close $out_398 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_398, 0;
    $result_398
};
        if (($cword == $counter)) {
            __docker_complete_plugins_installed();
        }
    }
;
    return;
}

sub _docker_plugin_remove {
    $main_exit_code = system('bash', '_docker_plugin_rm') >> 8;
    return;
}

sub _docker_plugin_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_399, $out_399);
    my $pid_399 = open3($in_399, $out_399, '>&STDERR', 'compgen', '-W', "--force -f --help", '--', "$ENV{cur}");
    close $in_399 or croak 'Close failed: $OS_ERROR';
    my $result_399 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_399> };
    close $out_399 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_399, 0;
    $result_399
});
    } elsif (1) {
                __docker_complete_plugins_installed();
    }
;
    return;
}

sub _docker_plugin_set {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_400, $out_400);
    my $pid_400 = open3($in_400, $out_400, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_400 or croak 'Close failed: $OS_ERROR';
    my $result_400 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_400> };
    close $out_400 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_400, 0;
    $result_400
});
    } elsif (1) {
                my $counter = do {
    my ($in_401, $out_401);
    my $pid_401 = open3($in_401, $out_401, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_401 or croak 'Close failed: $OS_ERROR';
    my $result_401 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_401> };
    close $out_401 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_401, 0;
    $result_401
};
        if (($cword == $counter)) {
            __docker_complete_plugins_installed();
        }
    }
;
    return;
}

sub _docker_plugin_upgrade {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_402, $out_402);
    my $pid_402 = open3($in_402, $out_402, '>&STDERR', 'compgen', '-W', "--disable-content-trust --grant-all-permissions --help --skip-remote-check", '--', "$ENV{cur}");
    close $in_402 or croak 'Close failed: $OS_ERROR';
    my $result_402 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_402> };
    close $out_402 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_402, 0;
    $result_402
});
    } elsif (1) {
                my $counter = do {
    my ($in_403, $out_403);
    my $pid_403 = open3($in_403, $out_403, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_403 or croak 'Close failed: $OS_ERROR';
    my $result_403 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_403> };
    close $out_403 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_403, 0;
    $result_403
};
        if (($cword == $counter)) {
            __docker_complete_plugins_installed();
            $main_exit_code = system('__ltrim_colon_completions', "$ENV{cur}") >> 8;
}
        else {
            if (($cword == (eval { int($counter + 1) } // ""))) {
                my $plugin_images = (do {
    my ($in_404, $out_404);
    my $pid_404 = open3($in_404, $out_404, '>&STDERR', '__docker_plugins_installed');
    close $in_404 or croak 'Close failed: $OS_ERROR';
    my $result_404 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_404> };
    close $out_404 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_404, 0;
    $result_404
});
                @COMPREPLY = (do {
    my ($in_405, $out_405);
    my $pid_405 = open3($in_405, $out_405, '>&STDERR', 'compgen', '-S', q{:}, '-W', (scalar reverse( (scalar reverse ${plugin_images}) =~ s/^.*?://r ) =~ s/:.*?$//r), '--', "$ENV{cur}");
    close $in_405 or croak 'Close failed: $OS_ERROR';
    my $result_405 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_405> };
    close $out_405 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_405, 0;
    $result_405
});
                __docker_nospace();
            }
        }
    }
;
    return;
}

sub _docker_port {
    _docker_container_port();
    return;
}

sub _docker_ps {
    _docker_container_ls();
    return;
}

sub _docker_pull {
    _docker_image_pull();
    return;
}

sub _docker_push {
    _docker_image_push();
    return;
}

sub _docker_rename {
    _docker_container_rename();
    return;
}

sub _docker_restart {
    _docker_container_restart();
    return;
}

sub _docker_rm {
    _docker_container_rm();
    return;
}

sub _docker_rmi {
    _docker_image_rm();
    return;
}

sub _docker_run {
    _docker_container_run();
    return;
}

sub _docker_save {
    _docker_image_save();
    return;
}

sub _docker_secret {
    my $subcommands = "
		create
		inspect
		ls
		rm
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_406, $out_406);
    my $pid_406 = open3($in_406, $out_406, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_406 or croak 'Close failed: $OS_ERROR';
    my $result_406 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_406> };
    close $out_406 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_406, 0;
    $result_406
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_407, $out_407);
    my $pid_407 = open3($in_407, $out_407, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_407 or croak 'Close failed: $OS_ERROR';
    my $result_407 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_407> };
    close $out_407 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_407, 0;
    $result_407
});
    }
;
    return;
}

sub _docker_secret_create {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--driver' or "$ENV{prev}" eq '-d' or "$ENV{prev}" eq '--label' or "$ENV{prev}" eq '-l') {
        return;    } elsif ("$ENV{prev}" eq '--template-driver') {
                @COMPREPLY = (do {
    my ($in_408, $out_408);
    my $pid_408 = open3($in_408, $out_408, '>&STDERR', 'compgen', '-W', "golang", '--', "$ENV{cur}");
    close $in_408 or croak 'Close failed: $OS_ERROR';
    my $result_408 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_408> };
    close $out_408 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_408, 0;
    $result_408
});
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_409, $out_409);
    my $pid_409 = open3($in_409, $out_409, '>&STDERR', 'compgen', '-W', "--driver -d --help --label -l --template-driver", '--', "$ENV{cur}");
    close $in_409 or croak 'Close failed: $OS_ERROR';
    my $result_409 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_409> };
    close $out_409 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_409, 0;
    $result_409
});
    } elsif (1) {
                my $counter = do {
    my ($in_410, $out_410);
    my $pid_410 = open3($in_410, $out_410, '>&STDERR', '__docker_pos_first_nonflag', '--driver|-d|--label|-l|--template-driver');
    close $in_410 or croak 'Close failed: $OS_ERROR';
    my $result_410 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_410> };
    close $out_410 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_410, 0;
    $result_410
};
        if (($cword == (eval { int($counter + 1) } // ""))) {
            $main_exit_code = system('bash', '_filedir') >> 8;
        }
    }
    return;
}

sub _docker_secret_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_411, $out_411);
    my $pid_411 = open3($in_411, $out_411, '>&STDERR', 'compgen', '-W', "--format -f --help --pretty", '--', "$ENV{cur}");
    close $in_411 or croak 'Close failed: $OS_ERROR';
    my $result_411 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_411> };
    close $out_411 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_411, 0;
    $result_411
});
    } elsif (1) {
                __docker_complete_secrets();
    }
;
    return;
}

sub _docker_secret_list {
    $main_exit_code = system('bash', '_docker_secret_ls') >> 8;
    return;
}

sub _docker_secret_ls {
    my $key = do {
    my ($in_412, $out_412);
    my $pid_412 = open3($in_412, $out_412, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_412 or croak 'Close failed: $OS_ERROR';
    my $result_412 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_412> };
    close $out_412 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_412, 0;
    $result_412
};
if ("$key" eq 'id') {
                __docker_complete_secrets('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_secrets('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    }
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_413, $out_413);
    my $pid_413 = open3($in_413, $out_413, '>&STDERR', 'compgen', '-S', q{=}, '-W', "id label name", '--', "$ENV{cur}");
    close $in_413 or croak 'Close failed: $OS_ERROR';
    my $result_413 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_413> };
    close $out_413 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_413, 0;
    $result_413
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_414, $out_414);
    my $pid_414 = open3($in_414, $out_414, '>&STDERR', 'compgen', '-W', "--format --filter -f --help --quiet -q", '--', "$ENV{cur}");
    close $in_414 or croak 'Close failed: $OS_ERROR';
    my $result_414 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_414> };
    close $out_414 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_414, 0;
    $result_414
});
    }
    return;
}

sub _docker_secret_remove {
    $main_exit_code = system('bash', '_docker_secret_rm') >> 8;
    return;
}

sub _docker_secret_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_415, $out_415);
    my $pid_415 = open3($in_415, $out_415, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_415 or croak 'Close failed: $OS_ERROR';
    my $result_415 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_415> };
    close $out_415 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_415, 0;
    $result_415
});
    } elsif (1) {
                __docker_complete_secrets();
    }
;
    return;
}

sub _docker_search {
    my $key = do {
    my ($in_416, $out_416);
    my $pid_416 = open3($in_416, $out_416, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_416 or croak 'Close failed: $OS_ERROR';
    my $result_416 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_416> };
    close $out_416 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_416, 0;
    $result_416
};
    my $COMPREPLY;
if ("$key" eq 'is-automated') {
                @COMPREPLY = (do {
    my ($in_417, $out_417);
    my $pid_417 = open3($in_417, $out_417, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_417 or croak 'Close failed: $OS_ERROR';
    my $result_417 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_417> };
    close $out_417 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_417, 0;
    $result_417
});
        return;    } elsif ("$key" eq 'is-official') {
                @COMPREPLY = (do {
    my ($in_418, $out_418);
    my $pid_418 = open3($in_418, $out_418, '>&STDERR', 'compgen', '-W', "false true", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_418 or croak 'Close failed: $OS_ERROR';
    my $result_418 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_418> };
    close $out_418 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_418, 0;
    $result_418
});
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_419, $out_419);
    my $pid_419 = open3($in_419, $out_419, '>&STDERR', 'compgen', '-S', q{=}, '-W', "is-automated is-official stars", '--', "$ENV{cur}");
    close $in_419 or croak 'Close failed: $OS_ERROR';
    my $result_419 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_419> };
    close $out_419 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_419, 0;
    $result_419
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '--limit') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_420, $out_420);
    my $pid_420 = open3($in_420, $out_420, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --limit --no-trunc", '--', "$ENV{cur}");
    close $in_420 or croak 'Close failed: $OS_ERROR';
    my $result_420 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_420> };
    close $out_420 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_420, 0;
    $result_420
});
    }
    return;
}

sub _docker_stack {
    my $subcommands = "
		config
		deploy
		ls
		ps
		rm
		services
	";
    my $aliases = "
		down
		list
		remove
		up
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_421, $out_421);
    my $pid_421 = open3($in_421, $out_421, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_421 or croak 'Close failed: $OS_ERROR';
    my $result_421 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_421> };
    close $out_421 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_421, 0;
    $result_421
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_422, $out_422);
    my $pid_422 = open3($in_422, $out_422, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_422 or croak 'Close failed: $OS_ERROR';
    my $result_422 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_422> };
    close $out_422 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_422, 0;
    $result_422
});
    }
;
    return;
}

sub _docker_stack_config {
if ("$ENV{prev}" eq '--compose-file' or "$ENV{prev}" eq '-c') {
                $main_exit_code = system('_filedir', 'yml') >> 8;
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_423, $out_423);
    my $pid_423 = open3($in_423, $out_423, '>&STDERR', 'compgen', '-W', "--compose-file -c --help --skip-interpolation", '--', "$ENV{cur}");
    close $in_423 or croak 'Close failed: $OS_ERROR';
    my $result_423 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_423> };
    close $out_423 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_423, 0;
    $result_423
});
    }
;
    return;
}

sub _docker_stack_deploy {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--compose-file' or "$ENV{prev}" eq '-c') {
                $main_exit_code = system('_filedir', 'yml') >> 8;
        return;    } elsif ("$ENV{prev}" eq '--resolve-image') {
                @COMPREPLY = (do {
    my ($in_424, $out_424);
    my $pid_424 = open3($in_424, $out_424, '>&STDERR', 'compgen', '-W', "always changed never", '--', "$ENV{cur}");
    close $in_424 or croak 'Close failed: $OS_ERROR';
    my $result_424 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_424> };
    close $out_424 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_424, 0;
    $result_424
});
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_425, $out_425);
    my $pid_425 = open3($in_425, $out_425, '>&STDERR', 'compgen', '-W', "--compose-file -c --help --prune --resolve-image --with-registry-auth", '--', "$ENV{cur}");
    close $in_425 or croak 'Close failed: $OS_ERROR';
    my $result_425 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_425> };
    close $out_425 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_425, 0;
    $result_425
});
    } elsif (1) {
                my $counter = do {
    my ($in_426, $out_426);
    my $pid_426 = open3($in_426, $out_426, '>&STDERR', '__docker_pos_first_nonflag', '--compose-file|-c|--resolve-image');
    close $in_426 or croak 'Close failed: $OS_ERROR';
    my $result_426 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_426> };
    close $out_426 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_426, 0;
    $result_426
};
        if (($cword == $counter)) {
            __docker_complete_stacks();
        }
    }
    return;
}

sub _docker_stack_down {
    $main_exit_code = system('bash', '_docker_stack_rm') >> 8;
    return;
}

sub _docker_stack_list {
    $main_exit_code = system('bash', '_docker_stack_ls') >> 8;
    return;
}

sub _docker_stack_ls {
if ("$ENV{prev}" eq '--format') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_427, $out_427);
    my $pid_427 = open3($in_427, $out_427, '>&STDERR', 'compgen', '-W', "--format --help", '--', "$ENV{cur}");
    close $in_427 or croak 'Close failed: $OS_ERROR';
    my $result_427 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_427> };
    close $out_427 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_427, 0;
    $result_427
});
    }
;
    return;
}

sub _docker_stack_ps {
    my $key = do {
    my ($in_428, $out_428);
    my $pid_428 = open3($in_428, $out_428, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_428 or croak 'Close failed: $OS_ERROR';
    my $result_428 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_428> };
    close $out_428 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_428, 0;
    $result_428
};
    my $COMPREPLY;
if ("$key" eq 'desired-state') {
                @COMPREPLY = (do {
    my ($in_429, $out_429);
    my $pid_429 = open3($in_429, $out_429, '>&STDERR', 'compgen', '-W', "accepted running shutdown", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_429 or croak 'Close failed: $OS_ERROR';
    my $result_429 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_429> };
    close $out_429 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_429, 0;
    $result_429
});
        return;    } elsif ("$key" eq 'id') {
                __docker_complete_stacks('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_stacks('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_430, $out_430);
    my $pid_430 = open3($in_430, $out_430, '>&STDERR', 'compgen', '-S', q{=}, '-W', "id name desired-state", '--', "$ENV{cur}");
    close $in_430 or croak 'Close failed: $OS_ERROR';
    my $result_430 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_430> };
    close $out_430 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_430, 0;
    $result_430
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_431, $out_431);
    my $pid_431 = open3($in_431, $out_431, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --no-resolve --no-trunc --quiet -q", '--', "$ENV{cur}");
    close $in_431 or croak 'Close failed: $OS_ERROR';
    my $result_431 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_431> };
    close $out_431 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_431, 0;
    $result_431
});
    } elsif (1) {
                my $counter = do {
    my ($in_432, $out_432);
    my $pid_432 = open3($in_432, $out_432, '>&STDERR', '__docker_pos_first_nonflag', '--filter|-f|--format');
    close $in_432 or croak 'Close failed: $OS_ERROR';
    my $result_432 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_432> };
    close $out_432 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_432, 0;
    $result_432
};
        if (($cword == $counter)) {
            __docker_complete_stacks();
        }
    }
    return;
}

sub _docker_stack_remove {
    $main_exit_code = system('bash', '_docker_stack_rm') >> 8;
    return;
}

sub _docker_stack_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_433, $out_433);
    my $pid_433 = open3($in_433, $out_433, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_433 or croak 'Close failed: $OS_ERROR';
    my $result_433 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_433> };
    close $out_433 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_433, 0;
    $result_433
});
    } elsif (1) {
                __docker_complete_stacks();
    }
;
    return;
}

sub _docker_stack_services {
    my $key = do {
    my ($in_434, $out_434);
    my $pid_434 = open3($in_434, $out_434, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_434 or croak 'Close failed: $OS_ERROR';
    my $result_434 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_434> };
    close $out_434 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_434, 0;
    $result_434
};
if ("$key" eq 'id') {
                __docker_complete_services('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--id');
        return;    } elsif ("$key" eq 'label') {
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_services('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--name');
        return;    }
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_435, $out_435);
    my $pid_435 = open3($in_435, $out_435, '>&STDERR', 'compgen', '-S', q{=}, '-W', "id label name", '--', "$ENV{cur}");
    close $in_435 or croak 'Close failed: $OS_ERROR';
    my $result_435 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_435> };
    close $out_435 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_435, 0;
    $result_435
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_436, $out_436);
    my $pid_436 = open3($in_436, $out_436, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --quiet -q", '--', "$ENV{cur}");
    close $in_436 or croak 'Close failed: $OS_ERROR';
    my $result_436 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_436> };
    close $out_436 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_436, 0;
    $result_436
});
    } elsif (1) {
                my $counter = do {
    my ($in_437, $out_437);
    my $pid_437 = open3($in_437, $out_437, '>&STDERR', '__docker_pos_first_nonflag', '--filter|-f|--format');
    close $in_437 or croak 'Close failed: $OS_ERROR';
    my $result_437 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_437> };
    close $out_437 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_437, 0;
    $result_437
};
        if (($cword == $counter)) {
            __docker_complete_stacks();
        }
    }
    return;
}

sub _docker_stack_up {
    _docker_stack_deploy();
    return;
}

sub _docker_start {
    _docker_container_start();
    return;
}

sub _docker_stats {
    _docker_container_stats();
    return;
}

sub _docker_stop {
    _docker_container_stop();
    return;
}

sub _docker_system {
    my $subcommands = "
		df
		events
		info
		prune
	";
    if (do {
__docker_subcommands("$subcommands");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_438, $out_438);
    my $pid_438 = open3($in_438, $out_438, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_438 or croak 'Close failed: $OS_ERROR';
    my $result_438 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_438> };
    close $out_438 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_438, 0;
    $result_438
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_439, $out_439);
    my $pid_439 = open3($in_439, $out_439, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_439 or croak 'Close failed: $OS_ERROR';
    my $result_439 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_439> };
    close $out_439 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_439, 0;
    $result_439
});
    }
;
    return;
}

sub _docker_system_df {
if ("$ENV{prev}" eq '--format') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_440, $out_440);
    my $pid_440 = open3($in_440, $out_440, '>&STDERR', 'compgen', '-W', "--format --help --verbose -v", '--', "$ENV{cur}");
    close $in_440 or croak 'Close failed: $OS_ERROR';
    my $result_440 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_440> };
    close $out_440 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_440, 0;
    $result_440
});
    }
;
    return;
}

sub _docker_system_events {
    my $key = do {
    my ($in_441, $out_441);
    my $pid_441 = open3($in_441, $out_441, '>&STDERR', '__docker_map_key_of_current_option', '-f|--filter');
    close $in_441 or croak 'Close failed: $OS_ERROR';
    my $result_441 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_441> };
    close $out_441 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_441, 0;
    $result_441
};
    my $COMPREPLY;
if ("$key" eq 'container') {
                __docker_complete_containers_all('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    } elsif ("$key" eq 'daemon') {
                my $name = do { my $result_442 = qx{bash -c '__docker_q info | sed -n "s/^\\\\(ID\\\\|Name\\\\): //p"' }; chomp $result_442; $result_442; };
                @COMPREPLY = (do {
    my ($in_443, $out_443);
    my $pid_443 = open3($in_443, $out_443, '>&STDERR', 'compgen', '-W', "$name", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_443 or croak 'Close failed: $OS_ERROR';
    my $result_443 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_443> };
    close $out_443 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_443, 0;
    $result_443
});
        return;    } elsif ("$key" eq 'event') {
                @COMPREPLY = (do {
    my ($in_444, $out_444);
    my $pid_444 = open3($in_444, $out_444, '>&STDERR', 'compgen', '-W', "
				attach
				commit
				connect
				copy
				create
				delete
				destroy
				detach
				die
				disable
				disconnect
				enable
				exec_create
				exec_detach
				exec_die
				exec_start
				export
				health_status
				import
				install
				kill
				load
				mount
				oom
				pause
				pull
				push
				reload
				remove
				rename
				resize
				restart
				save
				start
				stop
				tag
				top
				unmount
				unpause
				untag
				update
			", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_444 or croak 'Close failed: $OS_ERROR';
    my $result_444 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_444> };
    close $out_444 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_444, 0;
    $result_444
});
        return;    } elsif ("$key" eq 'image') {
                __docker_complete_images('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--repo', '--tag');
        return;    } elsif ("$key" eq 'network') {
                __docker_complete_networks('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    } elsif ("$key" eq 'node') {
                __docker_complete_nodes('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    } elsif ("$key" eq 'scope') {
                @COMPREPLY = (do {
    my ($in_445, $out_445);
    my $pid_445 = open3($in_445, $out_445, '>&STDERR', 'compgen', '-W', "local swarm", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_445 or croak 'Close failed: $OS_ERROR';
    my $result_445 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_445> };
    close $out_445 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_445, 0;
    $result_445
});
        return;    } elsif ("$key" eq 'type') {
                @COMPREPLY = (do {
    my ($in_446, $out_446);
    my $pid_446 = open3($in_446, $out_446, '>&STDERR', 'compgen', '-W', "config container daemon image network node plugin secret service volume", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_446 or croak 'Close failed: $OS_ERROR';
    my $result_446 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_446> };
    close $out_446 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_446, 0;
    $result_446
});
        return;    } elsif ("$key" eq 'volume') {
                __docker_complete_volumes('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_447, $out_447);
    my $pid_447 = open3($in_447, $out_447, '>&STDERR', 'compgen', '-S', q{=}, '-W', "container daemon event image label network node scope type volume", '--', "$ENV{cur}");
    close $in_447 or croak 'Close failed: $OS_ERROR';
    my $result_447 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_447> };
    close $out_447 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_447, 0;
    $result_447
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--since' or "$ENV{prev}" eq '--until') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_448, $out_448);
    my $pid_448 = open3($in_448, $out_448, '>&STDERR', 'compgen', '-W', "--filter -f --help --since --until --format", '--', "$ENV{cur}");
    close $in_448 or croak 'Close failed: $OS_ERROR';
    my $result_448 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_448> };
    close $out_448 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_448, 0;
    $result_448
});
    }
    return;
}

sub _docker_system_info {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_449, $out_449);
    my $pid_449 = open3($in_449, $out_449, '>&STDERR', 'compgen', '-W', "--format -f --help", '--', "$ENV{cur}");
    close $in_449 or croak 'Close failed: $OS_ERROR';
    my $result_449 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_449> };
    close $out_449 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_449, 0;
    $result_449
});
    }
;
    return;
}

sub _docker_system_prune {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter') {
                @COMPREPLY = (do {
    my ($in_450, $out_450);
    my $pid_450 = open3($in_450, $out_450, '>&STDERR', 'compgen', '-W', "label label! until", '-S', q{=}, '--', "$ENV{cur}");
    close $in_450 or croak 'Close failed: $OS_ERROR';
    my $result_450 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_450> };
    close $out_450 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_450, 0;
    $result_450
});
                __docker_nospace();
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_451, $out_451);
    my $pid_451 = open3($in_451, $out_451, '>&STDERR', 'compgen', '-W', "--all -a --force -f --filter --help --volumes", '--', "$ENV{cur}");
    close $in_451 or croak 'Close failed: $OS_ERROR';
    my $result_451 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_451> };
    close $out_451 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_451, 0;
    $result_451
});
    }
    return;
}

sub _docker_tag {
    _docker_image_tag();
    return;
}

sub _docker_trust {
    my $subcommands = "
		inspect
		revoke
		sign
	";
    if (do {
__docker_subcommands("$subcommands");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_452, $out_452);
    my $pid_452 = open3($in_452, $out_452, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_452 or croak 'Close failed: $OS_ERROR';
    my $result_452 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_452> };
    close $out_452 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_452, 0;
    $result_452
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_453, $out_453);
    my $pid_453 = open3($in_453, $out_453, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_453 or croak 'Close failed: $OS_ERROR';
    my $result_453 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_453> };
    close $out_453 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_453, 0;
    $result_453
});
    }
;
    return;
}

sub _docker_trust_inspect {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_454, $out_454);
    my $pid_454 = open3($in_454, $out_454, '>&STDERR', 'compgen', '-W', "--help --pretty", '--', "$ENV{cur}");
    close $in_454 or croak 'Close failed: $OS_ERROR';
    my $result_454 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_454> };
    close $out_454 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_454, 0;
    $result_454
});
    } elsif (1) {
                my $counter = do {
    my ($in_455, $out_455);
    my $pid_455 = open3($in_455, $out_455, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_455 or croak 'Close failed: $OS_ERROR';
    my $result_455 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_455> };
    close $out_455 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_455, 0;
    $result_455
};
        if (($cword == $counter)) {
            __docker_complete_images('--repo', '--tag');
        }
    }
;
    return;
}

sub _docker_trust_revoke {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_456, $out_456);
    my $pid_456 = open3($in_456, $out_456, '>&STDERR', 'compgen', '-W', "--help --yes -y", '--', "$ENV{cur}");
    close $in_456 or croak 'Close failed: $OS_ERROR';
    my $result_456 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_456> };
    close $out_456 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_456, 0;
    $result_456
});
    } elsif (1) {
                my $counter = do {
    my ($in_457, $out_457);
    my $pid_457 = open3($in_457, $out_457, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_457 or croak 'Close failed: $OS_ERROR';
    my $result_457 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_457> };
    close $out_457 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_457, 0;
    $result_457
};
        if (($cword == $counter)) {
            __docker_complete_images('--repo', '--tag');
        }
    }
;
    return;
}

sub _docker_trust_sign {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_458, $out_458);
    my $pid_458 = open3($in_458, $out_458, '>&STDERR', 'compgen', '-W', "--help --local", '--', "$ENV{cur}");
    close $in_458 or croak 'Close failed: $OS_ERROR';
    my $result_458 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_458> };
    close $out_458 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_458, 0;
    $result_458
});
    } elsif (1) {
                my $counter = do {
    my ($in_459, $out_459);
    my $pid_459 = open3($in_459, $out_459, '>&STDERR', '__docker_pos_first_nonflag');
    close $in_459 or croak 'Close failed: $OS_ERROR';
    my $result_459 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_459> };
    close $out_459 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_459, 0;
    $result_459
};
        if (($cword == $counter)) {
            __docker_complete_images('--force-tag', '--id');
        }
    }
;
    return;
}

sub _docker_unpause {
    _docker_container_unpause();
    return;
}

sub _docker_update {
    _docker_container_update();
    return;
}

sub _docker_top {
    _docker_container_top();
    return;
}

sub _docker_version {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_460, $out_460);
    my $pid_460 = open3($in_460, $out_460, '>&STDERR', 'compgen', '-W', "--format -f --help", '--', "$ENV{cur}");
    close $in_460 or croak 'Close failed: $OS_ERROR';
    my $result_460 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_460> };
    close $out_460 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_460, 0;
    $result_460
});
    }
;
    return;
}

sub _docker_volume_create {
if ("$ENV{prev}" eq '--driver' or "$ENV{prev}" eq '-d') {
                __docker_complete_plugins_bundled('--type', 'Volume');
        return;    } elsif ("$ENV{prev}" eq '--label' or "$ENV{prev}" eq '--opt' or "$ENV{prev}" eq '-o') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_461, $out_461);
    my $pid_461 = open3($in_461, $out_461, '>&STDERR', 'compgen', '-W', "--driver -d --help --label --opt -o", '--', "$ENV{cur}");
    close $in_461 or croak 'Close failed: $OS_ERROR';
    my $result_461 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_461> };
    close $out_461 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_461, 0;
    $result_461
});
    }
;
    return;
}

sub _docker_volume_inspect {
if ("$ENV{prev}" eq '--format' or "$ENV{prev}" eq '-f') {
        return;    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_462, $out_462);
    my $pid_462 = open3($in_462, $out_462, '>&STDERR', 'compgen', '-W', "--format -f --help", '--', "$ENV{cur}");
    close $in_462 or croak 'Close failed: $OS_ERROR';
    my $result_462 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_462> };
    close $out_462 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_462, 0;
    $result_462
});
    } elsif (1) {
                __docker_complete_volumes();
    }
;
    return;
}

sub _docker_volume_list {
    $main_exit_code = system('bash', '_docker_volume_ls') >> 8;
    return;
}

sub _docker_volume_ls {
    my $key = do {
    my ($in_463, $out_463);
    my $pid_463 = open3($in_463, $out_463, '>&STDERR', '__docker_map_key_of_current_option', '--filter|-f');
    close $in_463 or croak 'Close failed: $OS_ERROR';
    my $result_463 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_463> };
    close $out_463 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_463, 0;
    $result_463
};
    my $COMPREPLY;
if ("$key" eq 'dangling') {
                @COMPREPLY = (do {
    my ($in_464, $out_464);
    my $pid_464 = open3($in_464, $out_464, '>&STDERR', 'compgen', '-W', "true false", '--', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
    close $in_464 or croak 'Close failed: $OS_ERROR';
    my $result_464 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_464> };
    close $out_464 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_464, 0;
    $result_464
});
        return;    } elsif ("$key" eq 'driver') {
                __docker_complete_plugins_bundled('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr), '--type', 'Volume');
        return;    } elsif ("$key" eq 'name') {
                __docker_complete_volumes('--cur', (($ENV{cur} // q{}) =~ s/^.*=//sr =~ s/^.*=//sr));
        return;    }
;
if ("$ENV{prev}" eq '--filter' or "$ENV{prev}" eq '-f') {
                @COMPREPLY = (do {
    my ($in_465, $out_465);
    my $pid_465 = open3($in_465, $out_465, '>&STDERR', 'compgen', '-S', q{=}, '-W', "dangling driver label name", '--', "$ENV{cur}");
    close $in_465 or croak 'Close failed: $OS_ERROR';
    my $result_465 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_465> };
    close $out_465 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_465, 0;
    $result_465
});
                __docker_nospace();
        return;    } elsif ("$ENV{prev}" eq '--format') {
        return;    }
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_466, $out_466);
    my $pid_466 = open3($in_466, $out_466, '>&STDERR', 'compgen', '-W', "--filter -f --format --help --quiet -q", '--', "$ENV{cur}");
    close $in_466 or croak 'Close failed: $OS_ERROR';
    my $result_466 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_466> };
    close $out_466 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_466, 0;
    $result_466
});
    }
    return;
}

sub _docker_volume_prune {
    my $COMPREPLY;
if ("$ENV{prev}" eq '--filter') {
                @COMPREPLY = (do {
    my ($in_467, $out_467);
    my $pid_467 = open3($in_467, $out_467, '>&STDERR', 'compgen', '-W', "label label!", '-S', q{=}, '--', "$ENV{cur}");
    close $in_467 or croak 'Close failed: $OS_ERROR';
    my $result_467 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_467> };
    close $out_467 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_467, 0;
    $result_467
});
                __docker_nospace();
        return;    }
;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_468, $out_468);
    my $pid_468 = open3($in_468, $out_468, '>&STDERR', 'compgen', '-W', "--all -a --filter --force -f --help", '--', "$ENV{cur}");
    close $in_468 or croak 'Close failed: $OS_ERROR';
    my $result_468 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_468> };
    close $out_468 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_468, 0;
    $result_468
});
    }
    return;
}

sub _docker_volume_remove {
    $main_exit_code = system('bash', '_docker_volume_rm') >> 8;
    return;
}

sub _docker_volume_rm {
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_469, $out_469);
    my $pid_469 = open3($in_469, $out_469, '>&STDERR', 'compgen', '-W', "--force -f --help", '--', "$ENV{cur}");
    close $in_469 or croak 'Close failed: $OS_ERROR';
    my $result_469 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_469> };
    close $out_469 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_469, 0;
    $result_469
});
    } elsif (1) {
                __docker_complete_volumes();
    }
;
    return;
}

sub _docker_volume {
    my $subcommands = "
		create
		inspect
		ls
		prune
		rm
	";
    my $aliases = "
		list
		remove
	";
    if (do {
__docker_subcommands("$subcommands $aliases");
        $CHILD_ERROR == 0
    }) {
        return;
    }
    my $COMPREPLY;
if ("$ENV{cur}" =~ /^-.*$/msx) {
                @COMPREPLY = (do {
    my ($in_470, $out_470);
    my $pid_470 = open3($in_470, $out_470, '>&STDERR', 'compgen', '-W', "--help", '--', "$ENV{cur}");
    close $in_470 or croak 'Close failed: $OS_ERROR';
    my $result_470 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_470> };
    close $out_470 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_470, 0;
    $result_470
});
    } elsif (1) {
                @COMPREPLY = (do {
    my ($in_471, $out_471);
    my $pid_471 = open3($in_471, $out_471, '>&STDERR', 'compgen', '-W', "$subcommands", '--', "$ENV{cur}");
    close $in_471 or croak 'Close failed: $OS_ERROR';
    my $result_471 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_471> };
    close $out_471 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_471, 0;
    $result_471
});
    }
;
    return;
}

sub _docker_wait {
    _docker_container_wait();
    return;
}

sub _docker {
    my $previous_extglob_setting = ('shopt -p extglob');
# extglob option enabled
    my @management_commands = ('builder', 'config', 'container', 'context', 'image', 'manifest', 'network', 'node', 'plugin', 'secret', 'service', 'stack', 'swarm', 'system', 'trust', 'volume');
    my @top_level_commands = ('build', 'login', 'logout', 'run', 'search', 'version');
    my @legacy_commands = ('attach', 'commit', 'cp', 'create', 'diff', 'events', 'exec', 'export', 'history', 'images', 'import', 'info', 'inspect', 'kill', 'load', 'logs', 'pause', 'port', 'ps', 'pull', 'push', 'rename', 'restart', 'rm', 'rmi', 'save', 'start', 'stats', 'stop', 'tag', 'top', 'unpause', 'update', 'wait');
    my @known_plugin_commands = ();
    my $plugin_name = "";
    my $plugin_path;
    for my $plugin_path (do {
    my ($in_472, $out_472);
    my $pid_472 = open3($in_472, $out_472, '>&STDERR', '__docker_plugins_path');
    close $in_472 or croak 'Close failed: $OS_ERROR';
    my $result_472 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_472> };
    close $out_472 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_472, 0;
    $result_472
}) {
        $plugin_name = do { my $result_473 = qx{bash -c q{basename "$plugin_path" | sed 's/ *$//'} }; chomp $result_473; $result_473; };
        $plugin_name = ${plugin_name} =~ s/^docker-//r;
        $plugin_name = ${plugin_name} =~ s/\..*$//sr;
do { my $eval_input = "_docker_" . ${plugin_name} . "() { __docker_complete_plugin \"" . ${plugin_path} . "\"; }"; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
        push @known_plugin_commands, ${plugin_name};
    }
;
    my @experimental_server_commands = ('checkpoint');
    my @commands = ('${management_commands[*]}', '${top_level_commands[*]}', '${known_plugin_commands[*]}');
    if ("${DOCKER_HIDE_LEGACY_COMMANDS-}" eq q{}) {
                @commands = ($legacy_commands[eval { int(*) } // ""]);
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    my $global_boolean_options = "
		--debug -D
		--tls
		--tlsverify
	";
    my $global_options_with_args = "
		--config
		--context -c
		--host -H
		--log-level -l
		--tlscacert
		--tlscert
		--tlskey
	";
    my $info_fetched;
    my $server_experimental;
    my $server_os;
    my $host;
    my $config;
    my $context;
    my @COMPREPLY = ();
    my $cur;
    my $prev;
    my $words;
    my $cword;
    $main_exit_code = system('_get_comp_words_by_ref', '-n', q{:}, 'cur', 'prev', 'words', 'cword') >> 8;
    my $command = "docker";
    my $command_pos = "0";
    my $subcommand_pos;
    my $counter = "1";
while ( $counter < $cword ) {
if ($words[eval { int($counter) } // ""] eq 'docker') {
            return q{0};        } elsif ($words[eval { int($counter) } // ""] eq '--host' or $words[eval { int($counter) } // ""] eq '-H') {
                        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
                        $host = $words[eval { int($counter) } // ""];
        } elsif ($words[eval { int($counter) } // ""] eq '--config') {
                        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
                        $config = $words[eval { int($counter) } // ""];
        } elsif ($words[eval { int($counter) } // ""] eq '--context' or $words[eval { int($counter) } // ""] eq '-c') {
                        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
                        $context = $words[eval { int($counter) } // ""];
        } elsif ($words[eval { int($counter) } // ""] eq '$(__docker_to_extglob "$global_options_with_args")') {
                        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
        } elsif ($words[eval { int($counter) } // ""] =~ /^-.*$/msx) {
        } elsif ($words[eval { int($counter) } // ""] eq '=') {
                        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
        } elsif (1) {
                        $command = $words[eval { int($counter) } // ""];
                        $command_pos = $counter;
            last;        }
        $CHILD_ERROR = ($main_exit_code = eval { int($counter++) } // "") ? 0 : 1;
    }
    my $binary = $words[0];
if ($binary =~ /[?][(][*]\/[)]dockerd/msx) {
        $command = 'daemon';
        $command_pos = q{0};
    }
    my $completions_func = $command//-/_;
    if (do {
                do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', '/dev/null'
      or die "Cannot access file: $OS_ERROR\n";
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
    } == 0) {
                $CHILD_ERROR = 0;
    }
do { my $eval_input = $previous_extglob_setting; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
return q{0};
    return;
}
do { my $eval_input = $__docker_previous_extglob_setting; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
undef $__docker_previous_extglob_setting;
delete $ENV{__docker_previous_extglob_setting};
$main_exit_code = system('complete', '-F', '_docker', 'docker', 'docker.exe', 'dockerd', 'dockerd.exe') >> 8;

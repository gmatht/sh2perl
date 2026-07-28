#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);

my $main_exit_code = 0;
my $output         = q{};
our $CHILD_ERROR;

my $DATAFILE;
my $XSLTFILE;
my $DONE;
my $UNCOMPRESSED_SA_FILE;
my $GNUPLOTFILE;
my $SA_FILE;

$__set_e = 1;
my $ZENITY = do { my $which_cmd = 'which zenity'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $XSLTPROC = do { my $which_cmd = 'which xsltproc'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $SADF = do { my $which_cmd = 'which sadf'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $GNUPLOT = do { my $which_cmd = 'which gnuplot'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $MKTEMP = do { my $which_cmd = 'which mktemp'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $FIND = do { my $which_cmd = 'which find'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $SORT = do { my $which_cmd = 'which sort'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $CUT = do { my $which_cmd = 'which cut'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
my $GZIP = do { my $which_cmd = 'which gzip'; my $which_output = qx{$which_cmd}; $CHILD_ERROR = $? >> 8; $which_output; };
# set +e not implemented
my $SA_DIR = "/var/log/sysstat";
my $SA_REGEX = "/sa[0-9][0-9]+(\\.(gz|bz2|xz|lz|lzo))?\$";
$__set_e = 1;
my $parsed_opts = do {
    my ($in_0, $out_0);
    my $pid_0 = open3($in_0, $out_0, '>&STDERR', 'getopt', '-o', "", '-l', 'sa-dir:', '--', "\@ARGV");
    close $in_0 or croak 'Close failed: $OS_ERROR';
    my $result_0 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_0> };
    close $out_0 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_0, 0;
    $result_0
};
do { my $eval_input = "set" . "--" . $parsed_opts; system('bash', '-c', $eval_input); $CHILD_ERROR = $? >> 8; };
$DONE = 'no';
while ( $DONE ne yes ) {
if ($arg1 eq '--sa-dir') {
        # Builtin command 'shift' not implemented
                $SA_DIR = "$_[0]";
    } elsif ($arg1 eq '--') {
                $DONE = 'yes';
    } elsif (1) {
                say 'Unexpected' . q{ } . 'argument:' . q{ } . $1;
        exit 1;
    }
# Builtin command 'shift' not implemented
}
# set +e not implemented

sub cpu_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./cpu-load/cpu[@number='all']/@user\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./cpu-load/cpu[@number='all']/@nice\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./cpu-load/cpu[@number='all']/@system\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./cpu-load/cpu[@number='all']/@iowait\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./cpu-load/cpu[@number='all']/@steal\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub cpu_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -u\"
set ylabel \"Percent\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"%user\" with line, \"$2\" using 1:3 t \"%nice\" with line, \"$2\" using 1:4 t \"%system\" with line, \"$2\" using 1:5 t \"%iowait\" with line, \"$2\" using 1:6 t \"%steal\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub rq_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@runq-sz\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@plist-sz\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@ldavg-1\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@ldavg-5\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@ldavg-15\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub rq_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -q\"
set ylabel \"\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"runq-sz\" with line, \"$2\" using 1:3 t \"plist-sz\" with line, \"$2\" using 1:4 t \"ldavg-1\" with line, \"$2\" using 1:5 t \"ldavg-5\" with line, \"$2\" using 1:6 t \"ldavg-15\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub rqnoplistsz_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@runq-sz\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@ldavg-1\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@ldavg-5\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./queue/@ldavg-15\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub rqnoplistsz_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -q\"
set ylabel \"\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"runq-sz\" with line, \"$2\" using 1:3 t \"ldavg-1\" with line, \"$2\" using 1:4 t \"ldavg-5\" with line, \"$2\" using 1:5 t \"ldavg-15\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub io_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./io/tps\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./io/io-reads/@rtps\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./io/io-writes/@wtps\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./io/io-reads/@bread\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./io/io-writes/@bwrtn\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub io_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -b\"
set ylabel \"ops/s\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"rtps\" with line, \"$2\" using 1:3 t \"wtps\" with line, \"$2\" using 1:4 t \"bread/s\" with line,  \"$2\" using 1:5 t \"bwrtn/s\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub nfsclient_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./network/net-nfs/@call\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./network/net-nfs/@retrans\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./network/net-nfs/@read\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./network/net-nfs/@write\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./network/net-nfs/@access\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./network/net-nfs/@getatt\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub nfsclient_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -n NFS\"
set ylabel \"ops/s\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"call/s\" with line, \"$2\" using 1:3 t \"retrans/s\" with line, \"$2\" using 1:4 t \"read/s\" with line,  \"$2\" using 1:5 t \"write/s\" with line, \"$2\" using 1:6 t \"access/s\" with line, \"$2\" using 1:7 t \"getatt/s\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub paging_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./paging/@pgpgin\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./paging/@pgpgout\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./paging/@fault\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./paging/@majflt\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub paging_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -B\"
set ylabel \"pages/s\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"pgpgin/s\" with line, \"$2\" using 1:3 t \"pgpgout/s\" with line, \"$2\" using 1:4 t \"fault/s\" with line, \"$2\" using 1:5 t \"majflt/s\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub memuse_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/memfree\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/memused\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/buffers\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/cached\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/swpfree\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/swpused\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/swpcad\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub memuse_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -r\"
set ylabel \"kB\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"kbmemfree\" with line, \"$2\" using 1:3 t \"kbmemused\" with line, \"$2\" using 1:4 t \"kbbuffers\" with line, \"$2\" using 1:5 t \"kbcached\" with line, \"$2\" using 1:6 t \"swpfree\" with line, \"$2\" using 1:7 t \"swpused\" with line, \"$2\" using 1:8 t \"swpcad\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub swapuse_xslt {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">
<xsl:strip-space elements=\"*\"/>
<xsl:template match=\"/sysstat/host/statistics\">
<xsl:text>&#10;</xsl:text>
<xsl:for-each select=\"timestamp\">
<xsl:value-of select=\"@time\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/swpfree\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/swpused\"/>
<xsl:text> </xsl:text>
<xsl:value-of select=\"./memory/swpcad\"/>
<xsl:text>&#10;</xsl:text>
</xsl:for-each>
</xsl:template>
</xsl:stylesheet>
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}

sub swapuse_gnuplot {
open my $fh_cat, '>', '$1' or croak "Cannot access file: $OS_ERROR\n";
print {$fh_cat} "set term x11
set title \"sar -S\"
set ylabel \"kB\"
set timefmt \"%H:%M:%S\"
set xdata time
set format x \"%H:%M\"
plot \"$2\" using 1:2 t \"swpfree\" with line, \"$2\" using 1:3 t \"swpused\" with line, \"$2\" using 1:4 t \"swpcad\" with line
pause mouse
";
close $fh_cat or croak "Close failed: $OS_ERROR\n";
    return;
}
my $SA_FILES = do { my $result_1 = qx{bash -c '$FIND "$SA_DIR" -type f -p rintf "%T@,%p\\\\n" | grep -E "$SA_REGEX" | $SORT -n -r | $CUT -d , -f 2' }; chomp $result_1; $result_1; };
$DONE = 'no';
my $GRAPH;
while ( $DONE ne yes ) {
    $SA_FILE = do {
    my ($in_2, $out_2);
    my $pid_2 = open3($in_2, $out_2, '>&STDERR', $ZENITY, '--list', '--text', "Select data source", '--column', "sa file", $SA_FILES);
    close $in_2 or croak 'Close failed: $OS_ERROR';
    my $result_2 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_2> };
    close $out_2 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_2, 0;
    $result_2
};
if ("$SA_FILE" =~ /^""$/msx) {
exit $main_exit_code;
    }
if (!(    # Original bash: echo $SA_FILE | grep '.gz$'
do {
        my $output_3 = q{};
        my $output_printed_3;
        my $pipeline_success_3 = 1;
        $output_3 .= $SA_FILE . "\n";
if ( !($output_3 =~ m{\n\z}) ) { $output_3 .= "\n"; }

                my $grep_result_3_1;
        my @grep_lines_3_1 = split /\n/msx, $output_3;
        my @grep_filtered_3_1 = grep { /.gz$/msx } @grep_lines_3_1;
        $grep_result_3_1 = join "\n", @grep_filtered_3_1;
        if (!($grep_result_3_1 =~ m{\n\z} || $grep_result_3_1 eq q{})) {
        $grep_result_3_1 .= "\n";
        }
        $CHILD_ERROR = scalar @grep_filtered_3_1 > 0 ? 0 : 1;
        $output_3 = $grep_result_3_1;
        $output_3 = $grep_result_3_1;
        if ((scalar @grep_filtered_3_1) == 0) {
            $pipeline_success_3 = 0;
        }
        if ($output_3 ne q{} && !defined $output_printed_3) {
            print $output_3;
            if (!($output_3 =~ m{\n\z})) {
                print "\n";
            }
        }
        if ( !$pipeline_success_3 ) { $main_exit_code = 1; }
        };)) {
        $UNCOMPRESSED_SA_FILE = do {
    my ($in_4, $out_4);
    my $pid_4 = open3($in_4, $out_4, '>&STDERR', 'mktemp');
    close $in_4 or croak 'Close failed: $OS_ERROR';
    my $result_4 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_4> };
    close $out_4 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_4, 0;
    $result_4
};
        do {
            open my $original_stdout, '>&', STDOUT
      or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $UNCOMPRESSED_SA_FILE
      or die "Cannot access file: $OS_ERROR\n";
            my $tmp = do {
my @results;
if (-f q{c}) {
if (q{c}.gz =~ /[\[].]gz$/msx) {
my ($in_6);
my $pid_6 = open3($in_6, $out_6, $err_6, 'gunzip', '-c', 'q{c}.gz');
close $in_6 or croak 'Close failed: $OS_ERROR';
my $decompressed = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_6> };
close $out_6 or croak 'Close failed: $OS_ERROR';
waitpid $pid_6, 0;
if (defined $decompressed) {
push @results, "Decompressed: q{c}";
} else {
push @results, "Failed to decompress: q{c}";
}
} else {
push @results, "File not compressed: q{c}";
}
} else {
push @results, "File not found: q{c}";
}
if (-f $SA_FILE) {
if ($SA_FILE.gz =~ /[\[].]gz$/msx) {
my ($in_7);
my $pid_7 = open3($in_7, $out_7, $err_7, 'gunzip', '-c', '$SA_FILE.gz');
close $in_7 or croak 'Close failed: $OS_ERROR';
my $decompressed = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_7> };
close $out_7 or croak 'Close failed: $OS_ERROR';
waitpid $pid_7, 0;
if (defined $decompressed) {
push @results, "Decompressed: $SA_FILE";
} else {
push @results, "Failed to decompress: $SA_FILE";
}
} else {
push @results, "File not compressed: $SA_FILE";
}
} else {
push @results, "File not found: $SA_FILE";
}
 = join "\n", @results;

            };
            print $tmp;
            open STDOUT, '>&', $original_stdout
      or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
      or die "Close failed: $OS_ERROR\n";
        };
        $SA_FILE = $UNCOMPRESSED_SA_FILE;
    }
    $GRAPH = do {
    my ($in_8, $out_8);
    my $pid_8 = open3($in_8, $out_8, '>&STDERR', $ZENITY, '--list', '--text', "Select a graph", '--column', "Graph Type", "CPU", "Run Queue", "Run Queue w/o Process List Size", "IO Transfer Rate", "NFS Client", "Paging Stats", "Memory Utilization", "Memory Utilization (Swap)");
    close $in_8 or croak 'Close failed: $OS_ERROR';
    my $result_8 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_8> };
    close $out_8 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_8, 0;
    $result_8
};
if ("$GRAPH" eq 'CPU') {
                $XSLTFILE = do {
    my ($in_9, $out_9);
    my $pid_9 = open3($in_9, $out_9, '>&STDERR', 'mktemp');
    close $in_9 or croak 'Close failed: $OS_ERROR';
    my $result_9 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_9> };
    close $out_9 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_9, 0;
    $result_9
};
                cpu_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_10, $out_10);
    my $pid_10 = open3($in_10, $out_10, '>&STDERR', 'mktemp');
    close $in_10 or croak 'Close failed: $OS_ERROR';
    my $result_10 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_10> };
    close $out_10 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_10, 0;
    $result_10
};
                # Original bash: $SADF -t -x $SA_FILE -- -u | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_11 = q{};
            my $output_printed_11;
            my $pipeline_success_11 = 1;
                        my ($in_12, $out_12);
            my $pid_12 = open3($in_12, $out_12, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-u');
            close $in_12 or croak 'Close failed: $OS_ERROR';
            $output_11 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_12> };
            close $out_12 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_12, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_13 = q{};
            my $cmd_16 = 'unknown_command';
            my ($in_15, $out_15);
            my $pid_15 = open3($in_15, $out_15, '>&STDERR', $cmd_16, '--novalid', q{-});
            print {$in_15} $output_11;
            close $in_15 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_13 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_15> };
            close $out_15 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_15, 0;
            $tmp_redirect_13;
            $output_printed_11 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_11 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_17, $out_17);
    my $pid_17 = open3($in_17, $out_17, '>&STDERR', 'mktemp');
    close $in_17 or croak 'Close failed: $OS_ERROR';
    my $result_17 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_17> };
    close $out_17 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_17, 0;
    $result_17
};
                cpu_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif ("$GRAPH" eq 'Run Queue') {
                $XSLTFILE = do {
    my ($in_18, $out_18);
    my $pid_18 = open3($in_18, $out_18, '>&STDERR', 'mktemp');
    close $in_18 or croak 'Close failed: $OS_ERROR';
    my $result_18 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_18> };
    close $out_18 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_18, 0;
    $result_18
};
                rq_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_19, $out_19);
    my $pid_19 = open3($in_19, $out_19, '>&STDERR', 'mktemp');
    close $in_19 or croak 'Close failed: $OS_ERROR';
    my $result_19 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_19> };
    close $out_19 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_19, 0;
    $result_19
};
                # Original bash: $SADF -t -x $SA_FILE -- -q | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_20 = q{};
            my $output_printed_20;
            my $pipeline_success_20 = 1;
                        my ($in_21, $out_21);
            my $pid_21 = open3($in_21, $out_21, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-q');
            close $in_21 or croak 'Close failed: $OS_ERROR';
            $output_20 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_21> };
            close $out_21 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_21, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_22 = q{};
            my $cmd_25 = 'unknown_command';
            my ($in_24, $out_24);
            my $pid_24 = open3($in_24, $out_24, '>&STDERR', $cmd_25, '--novalid', q{-});
            print {$in_24} $output_20;
            close $in_24 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_22 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_24> };
            close $out_24 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_24, 0;
            $tmp_redirect_22;
            $output_printed_20 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_20 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_26, $out_26);
    my $pid_26 = open3($in_26, $out_26, '>&STDERR', 'mktemp');
    close $in_26 or croak 'Close failed: $OS_ERROR';
    my $result_26 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_26> };
    close $out_26 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_26, 0;
    $result_26
};
                rq_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif ("$GRAPH" eq 'Run Queue w/o Process List Size') {
                $XSLTFILE = do {
    my ($in_27, $out_27);
    my $pid_27 = open3($in_27, $out_27, '>&STDERR', 'mktemp');
    close $in_27 or croak 'Close failed: $OS_ERROR';
    my $result_27 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_27> };
    close $out_27 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_27, 0;
    $result_27
};
                rqnoplistsz_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_28, $out_28);
    my $pid_28 = open3($in_28, $out_28, '>&STDERR', 'mktemp');
    close $in_28 or croak 'Close failed: $OS_ERROR';
    my $result_28 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_28> };
    close $out_28 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_28, 0;
    $result_28
};
                # Original bash: $SADF -t -x $SA_FILE -- -q | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_29 = q{};
            my $output_printed_29;
            my $pipeline_success_29 = 1;
                        my ($in_30, $out_30);
            my $pid_30 = open3($in_30, $out_30, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-q');
            close $in_30 or croak 'Close failed: $OS_ERROR';
            $output_29 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_30> };
            close $out_30 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_30, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_31 = q{};
            my $cmd_34 = 'unknown_command';
            my ($in_33, $out_33);
            my $pid_33 = open3($in_33, $out_33, '>&STDERR', $cmd_34, '--novalid', q{-});
            print {$in_33} $output_29;
            close $in_33 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_31 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_33> };
            close $out_33 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_33, 0;
            $tmp_redirect_31;
            $output_printed_29 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_29 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_35, $out_35);
    my $pid_35 = open3($in_35, $out_35, '>&STDERR', 'mktemp');
    close $in_35 or croak 'Close failed: $OS_ERROR';
    my $result_35 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_35> };
    close $out_35 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_35, 0;
    $result_35
};
                rqnoplistsz_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif ("$GRAPH" eq 'IO Transfer Rate') {
                $XSLTFILE = do {
    my ($in_36, $out_36);
    my $pid_36 = open3($in_36, $out_36, '>&STDERR', 'mktemp');
    close $in_36 or croak 'Close failed: $OS_ERROR';
    my $result_36 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_36> };
    close $out_36 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_36, 0;
    $result_36
};
                io_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_37, $out_37);
    my $pid_37 = open3($in_37, $out_37, '>&STDERR', 'mktemp');
    close $in_37 or croak 'Close failed: $OS_ERROR';
    my $result_37 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_37> };
    close $out_37 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_37, 0;
    $result_37
};
                # Original bash: $SADF -t -x $SA_FILE -- -b | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_38 = q{};
            my $output_printed_38;
            my $pipeline_success_38 = 1;
                        my ($in_39, $out_39);
            my $pid_39 = open3($in_39, $out_39, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-b');
            close $in_39 or croak 'Close failed: $OS_ERROR';
            $output_38 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_39> };
            close $out_39 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_39, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_40 = q{};
            my $cmd_43 = 'unknown_command';
            my ($in_42, $out_42);
            my $pid_42 = open3($in_42, $out_42, '>&STDERR', $cmd_43, '--novalid', q{-});
            print {$in_42} $output_38;
            close $in_42 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_40 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_42> };
            close $out_42 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_42, 0;
            $tmp_redirect_40;
            $output_printed_38 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_38 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_44, $out_44);
    my $pid_44 = open3($in_44, $out_44, '>&STDERR', 'mktemp');
    close $in_44 or croak 'Close failed: $OS_ERROR';
    my $result_44 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_44> };
    close $out_44 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_44, 0;
    $result_44
};
                io_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif ("$GRAPH" eq 'NFS Client') {
                $XSLTFILE = do {
    my ($in_45, $out_45);
    my $pid_45 = open3($in_45, $out_45, '>&STDERR', 'mktemp');
    close $in_45 or croak 'Close failed: $OS_ERROR';
    my $result_45 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_45> };
    close $out_45 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_45, 0;
    $result_45
};
                nfsclient_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_46, $out_46);
    my $pid_46 = open3($in_46, $out_46, '>&STDERR', 'mktemp');
    close $in_46 or croak 'Close failed: $OS_ERROR';
    my $result_46 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_46> };
    close $out_46 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_46, 0;
    $result_46
};
                # Original bash: $SADF -t -x $SA_FILE -- -n NFS | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_47 = q{};
            my $output_printed_47;
            my $pipeline_success_47 = 1;
                        my ($in_48, $out_48);
            my $pid_48 = open3($in_48, $out_48, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-n', 'NFS');
            close $in_48 or croak 'Close failed: $OS_ERROR';
            $output_47 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_48> };
            close $out_48 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_48, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_49 = q{};
            my $cmd_52 = 'unknown_command';
            my ($in_51, $out_51);
            my $pid_51 = open3($in_51, $out_51, '>&STDERR', $cmd_52, '--novalid', q{-});
            print {$in_51} $output_47;
            close $in_51 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_49 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_51> };
            close $out_51 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_51, 0;
            $tmp_redirect_49;
            $output_printed_47 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_47 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_53, $out_53);
    my $pid_53 = open3($in_53, $out_53, '>&STDERR', 'mktemp');
    close $in_53 or croak 'Close failed: $OS_ERROR';
    my $result_53 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_53> };
    close $out_53 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_53, 0;
    $result_53
};
                nfsclient_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif ("$GRAPH" eq 'Paging Stats') {
                $XSLTFILE = do {
    my ($in_54, $out_54);
    my $pid_54 = open3($in_54, $out_54, '>&STDERR', 'mktemp');
    close $in_54 or croak 'Close failed: $OS_ERROR';
    my $result_54 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_54> };
    close $out_54 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_54, 0;
    $result_54
};
                paging_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_55, $out_55);
    my $pid_55 = open3($in_55, $out_55, '>&STDERR', 'mktemp');
    close $in_55 or croak 'Close failed: $OS_ERROR';
    my $result_55 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_55> };
    close $out_55 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_55, 0;
    $result_55
};
                # Original bash: $SADF -t -x $SA_FILE -- -B | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_56 = q{};
            my $output_printed_56;
            my $pipeline_success_56 = 1;
                        my ($in_57, $out_57);
            my $pid_57 = open3($in_57, $out_57, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-B');
            close $in_57 or croak 'Close failed: $OS_ERROR';
            $output_56 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_57> };
            close $out_57 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_57, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_58 = q{};
            my $cmd_61 = 'unknown_command';
            my ($in_60, $out_60);
            my $pid_60 = open3($in_60, $out_60, '>&STDERR', $cmd_61, '--novalid', q{-});
            print {$in_60} $output_56;
            close $in_60 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_58 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_60> };
            close $out_60 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_60, 0;
            $tmp_redirect_58;
            $output_printed_56 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_56 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_62, $out_62);
    my $pid_62 = open3($in_62, $out_62, '>&STDERR', 'mktemp');
    close $in_62 or croak 'Close failed: $OS_ERROR';
    my $result_62 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_62> };
    close $out_62 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_62, 0;
    $result_62
};
                paging_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif ("$GRAPH" eq 'Memory Utilization') {
                $XSLTFILE = do {
    my ($in_63, $out_63);
    my $pid_63 = open3($in_63, $out_63, '>&STDERR', 'mktemp');
    close $in_63 or croak 'Close failed: $OS_ERROR';
    my $result_63 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_63> };
    close $out_63 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_63, 0;
    $result_63
};
                memuse_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_64, $out_64);
    my $pid_64 = open3($in_64, $out_64, '>&STDERR', 'mktemp');
    close $in_64 or croak 'Close failed: $OS_ERROR';
    my $result_64 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_64> };
    close $out_64 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_64, 0;
    $result_64
};
                # Original bash: $SADF -t -x $SA_FILE -- -r | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_65 = q{};
            my $output_printed_65;
            my $pipeline_success_65 = 1;
                        my ($in_66, $out_66);
            my $pid_66 = open3($in_66, $out_66, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-r');
            close $in_66 or croak 'Close failed: $OS_ERROR';
            $output_65 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_66> };
            close $out_66 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_66, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_67 = q{};
            my $cmd_70 = 'unknown_command';
            my ($in_69, $out_69);
            my $pid_69 = open3($in_69, $out_69, '>&STDERR', $cmd_70, '--novalid', q{-});
            print {$in_69} $output_65;
            close $in_69 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_67 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_69> };
            close $out_69 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_69, 0;
            $tmp_redirect_67;
            $output_printed_65 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_65 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_71, $out_71);
    my $pid_71 = open3($in_71, $out_71, '>&STDERR', 'mktemp');
    close $in_71 or croak 'Close failed: $OS_ERROR';
    my $result_71 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_71> };
    close $out_71 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_71, 0;
    $result_71
};
                memuse_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif ("$GRAPH" eq 'Memory Utilization (Swap)') {
                $XSLTFILE = do {
    my ($in_72, $out_72);
    my $pid_72 = open3($in_72, $out_72, '>&STDERR', 'mktemp');
    close $in_72 or croak 'Close failed: $OS_ERROR';
    my $result_72 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_72> };
    close $out_72 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_72, 0;
    $result_72
};
                swapuse_xslt($XSLTFILE);
                $DATAFILE = do {
    my ($in_73, $out_73);
    my $pid_73 = open3($in_73, $out_73, '>&STDERR', 'mktemp');
    close $in_73 or croak 'Close failed: $OS_ERROR';
    my $result_73 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_73> };
    close $out_73 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_73, 0;
    $result_73
};
                # Original bash: $SADF -t -x $SA_FILE -- -S | $XSLTPROC --novalid $XSLTFILE - > $DATAFILE
do {
            my $output_74 = q{};
            my $output_printed_74;
            my $pipeline_success_74 = 1;
                        my ($in_75, $out_75);
            my $pid_75 = open3($in_75, $out_75, '>&STDERR', 'unknown_command', '-t', '-x', '--', '-S');
            close $in_75 or croak 'Close failed: $OS_ERROR';
            $output_74 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_75> };
            close $out_75 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_75, 0;

                        do {
            open my $original_stdout, '>&', STDOUT
            or die "Cannot save STDOUT: $OS_ERROR\n";
            open STDOUT, '>', $DATAFILE
            or die "Cannot access file: $OS_ERROR\n";
            my $tmp_redirect_76 = q{};
            my $cmd_79 = 'unknown_command';
            my ($in_78, $out_78);
            my $pid_78 = open3($in_78, $out_78, '>&STDERR', $cmd_79, '--novalid', q{-});
            print {$in_78} $output_74;
            close $in_78 or croak 'Close failed: $OS_ERROR';
            $tmp_redirect_76 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_78> };
            close $out_78 or croak 'Close failed: $OS_ERROR';
            waitpid $pid_78, 0;
            $tmp_redirect_76;
            $output_printed_74 = 1;
            open STDOUT, '>&', $original_stdout
            or die "Cannot restore STDOUT: $OS_ERROR\n";
            close $original_stdout
            or die "Close failed: $OS_ERROR\n";
            };
            if ( !$pipeline_success_74 ) { $main_exit_code = 1; }
            exit $main_exit_code if $__set_e && $main_exit_code != 0;
            }
                $GNUPLOTFILE = do {
    my ($in_80, $out_80);
    my $pid_80 = open3($in_80, $out_80, '>&STDERR', 'mktemp');
    close $in_80 or croak 'Close failed: $OS_ERROR';
    my $result_80 = do { local $INPUT_RECORD_SEPARATOR = undef; <$out_80> };
    close $out_80 or croak 'Close failed: $OS_ERROR';
    waitpid $pid_80, 0;
    $result_80
};
                swapuse_gnuplot($GNUPLOTFILE, $DATAFILE);
                $CHILD_ERROR = 0;
    } elsif (1) {
                $DONE = 'yes';
    }
    if ((-f "$UNCOMPRESSED_SA_FILE")) {
        if ( -e "$UNCOMPRESSED_SA_FILE" ) {
            if ( -d "$UNCOMPRESSED_SA_FILE" ) {
                croak "rm: ", $UNCOMPRESSED_SA_FILE,
          " is a directory (use -r to remove recursively)\n";
            }
            else {
                if ( unlink "$UNCOMPRESSED_SA_FILE" ) {
                                    }
                else {
                    croak "rm: cannot remove ", $UNCOMPRESSED_SA_FILE,
              ": $OS_ERROR\n";
                }
            }
        }
        else {
            local $CHILD_ERROR = 1;
            croak "rm: ", $UNCOMPRESSED_SA_FILE, ": No such file or directory\n";
        }
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    if ((-f "$GNUPLOTFILE")) {
        if ( -e "$GNUPLOTFILE" ) {
            if ( -d "$GNUPLOTFILE" ) {
                croak "rm: ", $GNUPLOTFILE,
          " is a directory (use -r to remove recursively)\n";
            }
            else {
                if ( unlink "$GNUPLOTFILE" ) {
                                    }
                else {
                    croak "rm: cannot remove ", $GNUPLOTFILE,
              ": $OS_ERROR\n";
                }
            }
        }
        else {
            local $CHILD_ERROR = 1;
            croak "rm: ", $GNUPLOTFILE, ": No such file or directory\n";
        }
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    if ((-f "$DATAFILE")) {
        if ( -e "$DATAFILE" ) {
            if ( -d "$DATAFILE" ) {
                croak "rm: ", $DATAFILE,
          " is a directory (use -r to remove recursively)\n";
            }
            else {
                if ( unlink "$DATAFILE" ) {
                                    }
                else {
                    croak "rm: cannot remove ", $DATAFILE,
              ": $OS_ERROR\n";
                }
            }
        }
        else {
            local $CHILD_ERROR = 1;
            croak "rm: ", $DATAFILE, ": No such file or directory\n";
        }
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
    if ((-f "$XSLTFILE")) {
        if ( -e "$XSLTFILE" ) {
            if ( -d "$XSLTFILE" ) {
                croak "rm: ", $XSLTFILE,
          " is a directory (use -r to remove recursively)\n";
            }
            else {
                if ( unlink "$XSLTFILE" ) {
                                    }
                else {
                    croak "rm: cannot remove ", $XSLTFILE,
              ": $OS_ERROR\n";
                }
            }
        }
        else {
            local $CHILD_ERROR = 1;
            croak "rm: ", $XSLTFILE, ": No such file or directory\n";
        }
        $CHILD_ERROR = 0;
    } else {
        $CHILD_ERROR = 1;
    }
}
exit $main_exit_code;

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use Term::ANSIColor qw(colored);

my @git_args = @ARGV ? @ARGV : ('-20');
my @pathspec = ('--', 'examples.out/');

my $format = '%H|%ad|%s|%an';
my $datefmt = '--date=format:%Y-%m-%d %H:%M';

my $output = '';

open my $lfh, '-|', 'git', 'log', @git_args, "--format=format:$format", $datefmt, @pathspec
    or die "Cannot run git log: $!";

my @entries;
while (<$lfh>) {
    chomp;
    my ($sha, $date, $subject, $author) = split /\|/, $_, 4;
    push @entries, { sha => $sha, date => $date, subject => $subject, author => $author };
}
close $lfh;

for my $e (@entries) {
    my $sha = $e->{sha};
    my $diff = `git diff '$sha^'..'$sha' -- 'examples.out/' 2>/dev/null`;
    my ($is_noise, $filtered) = filter_diff($diff);

    my $sha8 = substr($sha, 0, 8);
    if ($is_noise) {
        $output .= colored("$sha8  $e->{date}  $e->{subject}  $e->{author}", 'bright_black') . "\n";
    } elsif ($filtered) {
        $output .= colored($sha8, 'cyan') . '  '
                 . colored($e->{date}, 'blue') . '  '
                 . $e->{subject} . '  '
                 . colored($e->{author}, 'bright_black') . "\n";
        for my $l (split /\n/, $filtered) {
            if    ($l =~ /^diff --git/) { $output .= colored($l, 'magenta') . "\n" }
            elsif ($l =~ /^--- /)       { $output .= colored($l, 'red') . "\n" }
            elsif ($l =~ /^\+\+\+ /)    { $output .= colored($l, 'green') . "\n" }
            elsif ($l =~ /^@@ /)        { $output .= colored($l, 'cyan') . "\n" }
            elsif ($l =~ /^\+/)         { $output .= colored($l, 'green') . "\n" }
            elsif ($l =~ /^-/)          { $output .= colored($l, 'red') . "\n" }
            else                        { $output .= "$l\n" }
        }
        $output .= "\n";
    }
}

open my $less, '|-', 'less', '-R' or die "Cannot run less: $!";
print $less $output;
close $less;

sub filter_diff {
    my ($diff) = @_;
    return (1, '') unless $diff && $diff =~ /\S/;

    my $result = '';
    my $meaningful = 0;

    my @files = split_diff_by_file($diff);

    for my $file (@files) {
        my $fout = '';
        for my $hunk (@{$file->{hunks}}) {
            my @lines = @{$hunk->{lines}};
            my @filt;
            my $i = 0;

            while ($i < @lines) {
                if ($lines[$i] =~ /^ /) {
                    push @filt, $lines[$i]; $i++;
                } elsif ($lines[$i] =~ /^\-/) {
                    my @r;
                    while ($i < @lines && $lines[$i] =~ /^\-/) {
                        my $s = $lines[$i]; $s =~ s/^\-//;
                        push @r, $s; $i++;
                    }
                    my @a;
                    while ($i < @lines && $lines[$i] =~ /^\+/) {
                        my $s = $lines[$i]; $s =~ s/^\+//;
                        push @a, $s; $i++;
                    }
                    my $n = $#r < $#a ? $#r : $#a;
                    for my $j (0 .. $n) {
                        if (strip_numbers($r[$j]) eq strip_numbers($a[$j])) {
                            # noise — skip both
                        } else {
                            push @filt, '-' . $r[$j];
                            push @filt, '+' . $a[$j];
                            $meaningful++;
                        }
                    }
                    for my $j ($n + 1 .. $#r) { push @filt, '-' . $r[$j]; $meaningful++ }
                    for my $j ($n + 1 .. $#a) { push @filt, '+' . $a[$j]; $meaningful++ }
                } else { $i++ }
            }

            if (@filt) {
                my $oc = grep { /^[ -]/ } @filt;
                my $nc = grep { /^[+ ]/ } @filt;
                my ($oo) = $hunk->{raw} =~ /-(\d+)/;
                my ($no) = $hunk->{raw} =~ /\+(\d+)/;
                $fout .= "@@ -$oo,$oc +$no,$nc @@\n";
                for my $l (@filt) { $fout .= "$l\n" }
            }
        }
        if ($fout) {
            $result .= $file->{header} . "\n";
            $result .= $file->{old_file} . "\n";
            $result .= $file->{new_file} . "\n";
            $result .= $fout;
        }
    }
    return ($meaningful == 0, $result);
}

sub split_diff_by_file {
    my ($diff) = @_;
    my @files; my $c;
    for my $l (split /\n/, $diff) {
        if    ($l =~ /^diff --git/) { push @files, $c if $c; $c = { header => $l, old_file => '', new_file => '', hunks => [] } }
        elsif ($l =~ /^--- /)       { $c->{old_file} = $l if $c }
        elsif ($l =~ /^\+\+\+ /)    { $c->{new_file} = $l if $c }
        elsif ($l =~ /^@@ /)        { push @{$c->{hunks}}, { raw => $l, lines => [] } if $c }
        elsif ($l =~ /^[ +\-]/)     { push @{$c->{hunks}->[-1]{lines}}, $l if $c && @{$c->{hunks}} }
    }
    push @files, $c if $c;
    return @files;
}

sub strip_numbers {
    my ($s) = @_;
    $s =~ s/\d+/N/g;
    $s =~ s/_\d+/_N/g;
    return $s;
}

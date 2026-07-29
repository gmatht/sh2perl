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
    my ($is_noise, $filtered_diff) = filter_diff($diff);

    my $sha8 = substr($sha, 0, 8);
    if ($is_noise) {
        $output .= colored("$sha8  $e->{date}  $e->{subject}  $e->{author}", 'bright_black') . "\n";
    } elsif ($filtered_diff) {
        $output .= colored($sha8, 'cyan') . '  '
                 . colored($e->{date}, 'blue') . '  '
                 . $e->{subject} . '  '
                 . colored($e->{author}, 'bright_black') . "\n";
        for my $dline (split /\n/, $filtered_diff) {
            if    ($dline =~ /^diff --git/) { $output .= colored($dline, 'magenta') . "\n" }
            elsif ($dline =~ /^--- /)       { $output .= colored($dline, 'red') . "\n" }
            elsif ($dline =~ /^\+\+\+ /)    { $output .= colored($dline, 'green') . "\n" }
            elsif ($dline =~ /^@@ /)        { $output .= colored($dline, 'cyan') . "\n" }
            elsif ($dline =~ /^\+/)         { $output .= colored($dline, 'green') . "\n" }
            elsif ($dline =~ /^-/)          { $output .= colored($dline, 'red') . "\n" }
            else                            { $output .= "$dline\n" }
        }
        $output .= "\n";
    }
}

open my $less, '|-', 'less', '-R' or die "Cannot run less: $!";
print $less $output;
close $less;

# ── Filter: remove paired -/+ lines that differ only in numbers ──────

sub filter_diff {
    my ($diff) = @_;
    return (1, '') unless $diff && $diff =~ /\S/;

    my $result = '';
    my $meaningful = 0;

    my @files = split_diff_by_file($diff);

    for my $file (@files) {
        my $result_file = '';
        my @hunks = $file->{hunks};

        for my $hunk (@hunks) {
            my @lines   = @{$hunk->{lines}};
            my $raw_hdr = $hunk->{raw};   # @@ -old,count +new,count @@

            my @filtered;
            my $i = 0;
            while ($i < @lines) {
                my $line = $lines[$i];
                if ($line =~ /^ /) {
                    push @filtered, $line; $i++;
                } elsif ($line =~ /^-/) {
                    if ($i + 1 < @lines && $lines[$i + 1] =~ /^\+/) {
                        my $r = $line;  $r =~ s/^-//;
                        my $a = $lines[$i + 1]; $a =~ s/^\+//;
                        if (strip_numbers($r) eq strip_numbers($a)) {
                            $i += 2;  # skip both — noise
                        } else {
                            push @filtered, $line;
                            push @filtered, $lines[$i + 1];
                            $meaningful++;
                            $i += 2;
                        }
                    } else {
                        push @filtered, $line; $meaningful++; $i++;
                    }
                } elsif ($line =~ /^\+/) {
                    push @filtered, $line; $meaningful++; $i++;
                } else { $i++ }
            }

            if (@filtered > 0) {
                my $old_cnt = grep { /^[ -]/ } @filtered;
                my $new_cnt = grep { /^[+ ]/ } @filtered;
                my ($old_off) = $raw_hdr =~ /-(\d+)/;
                my ($new_off) = $raw_hdr =~ /\+(\d+)/;
                $result_file .= "@@ -$old_off,$old_cnt +$new_off,$new_cnt @@\n";
                for my $fline (@filtered) { $result_file .= "$fline\n" }
            }
        }

        if ($result_file) {
            $result .= $file->{header} . "\n";   # diff --git a/... b/...
            $result .= $file->{old_file} . "\n"; # --- a/...
            $result .= $file->{new_file} . "\n"; # +++ b/...
            $result .= $result_file;
        }
    }

    return ($meaningful == 0, $result);
}

sub split_diff_by_file {
    my ($diff) = @_;
    my @files;
    my $current;

    for my $line (split /\n/, $diff) {
        if ($line =~ /^diff --git/) {
            push @files, $current if $current;
            $current = { header => $line, old_file => '', new_file => '', hunks => [] };
        } elsif ($line =~ /^--- /) {
            $current->{old_file} = $line if $current;
        } elsif ($line =~ /^\+\+\+ /) {
            $current->{new_file} = $line if $current;
        } elsif ($line =~ /^@@ /) {
            push @{$current->{hunks}}, { raw => $line, lines => [] } if $current;
        } elsif ($line =~ /^[ +-]/) {
            push @{$current->{hunks}->[-1]{lines}}, $line if $current && @{$current->{hunks}};
        }
    }
    push @files, $current if $current;
    return @files;
}

sub strip_numbers {
    my ($s) = @_;
    $s =~ s/\d+/N/g;
    $s =~ s/_\d+/_N/g;
    return $s;
}

#!/usr/bin/env perl
use strict;
use warnings;
use Term::ANSIColor qw(colored);

# Colored git log for examples.out/.  Full patches for meaningful commits,
# dimmed one-liners for commits that only increment counter IDs.
#
# Usage: perl meangitlog.pl [git-log-options]

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
    my $is_noise = diff_is_counter_only($diff);

    my $sha8 = substr($sha, 0, 8);

    if ($is_noise) {
        $output .= colored("$sha8  $e->{date}  $e->{subject}  $e->{author}", 'bright_black') . "\n";
    } elsif ($diff) {
        $output .= colored($sha8, 'cyan') . '  '
                 . colored($e->{date}, 'blue') . '  '
                 . $e->{subject} . '  '
                 . colored($e->{author}, 'bright_black') . "\n";
        for my $dline (split /\n/, $diff) {
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

# ── Noise detection ──────────────────────────────────────────────────
# A diff is "counter-only" if every changed line differs only in its
# embedded numbers (unique IDs, counts, etc.).

sub diff_is_counter_only {
    my ($diff) = @_;
    return 0 unless $diff && $diff =~ /\S/;

    # Collect every - and + line content (strip the prefix)
    my @changed;
    for my $line (split /\n/, $diff) {
        next if $line =~ /^diff --git|^index |^--- |^\+\+\+ |^@@ |^[ -]$|^[ ]/;
        my $c = $line;
        $c =~ s/^[+-]//;
        next if $c =~ /^\s*$/;
        push @changed, $c;
    }

    return 0 if @changed == 0;

    # For each changed line, check if stripping numbers makes it empty
    for my $c (@changed) {
        my $stripped = strip_numbers($c);
        # After stripping numbers, is it empty or just whitespace/punctuation?
        $stripped =~ s/^\s+//;
        $stripped =~ s/\s+$//;
        # Also catch pure punctuation/symbols left after stripping: $, =, {, }, etc.
        my $clean = $stripped;
        $clean =~ s/[\[\];:.,{}\$=><!~\s\(\)]//g;
        if ($clean ne '') {
            return 0;  # non-numeric content remains — meaningful
        }
    }

    return 1;
}

sub strip_numbers {
    my ($s) = @_;
    $s =~ s/\d+/N/g;
    $s =~ s/_\d+/_N/g;
    return $s;
}

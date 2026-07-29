#!/usr/bin/env perl
use strict;
use warnings;
use Term::ANSIColor qw(colored);

# Colored git log showing changes to examples.out/, piped through less -R.
# Commits whose diff is ONLY numeric/ID changes are dimmed.
# Everything else shows its patch (-p style).
#
# Usage: perl meangitlog.pl [git-log-options]
#   Default: -20, showing only examples.out/ changes.

my @git_args = @ARGV ? @ARGV : ('-20');
my @pathspec = ('--', 'examples.out/');

my $format = '%H|%ad|%s|%an';
my $datefmt = '--date=format:%Y-%m-%d %H:%M';

# Collect all output first, then pipe through less -R
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
    my $is_noise = 1;

    if ($diff) {
        for my $dline (split /\n/, $diff) {
            next if $dline =~ /^diff --git|^index |^--- |^\+\+\+ |^@@ /;
            next if $dline =~ /^[ -]$/;
            next if $dline =~ /^[ ]/;
            next if $dline =~ /^[+-]\s*$/;

            my $c = $dline;
            $c =~ s/^[+-]\s*//;

            next if $c =~ /^\d+$/;
            next if $c =~ /^[a-zA-Z_]\w*_\d+$/;
            next if $c =~ /^(new|deleted) file mode/;
            next if $c =~ /^Binary files/;
            next if $c eq '';

            $is_noise = 0;
            last;
        }
    }

    my $sha8 = substr($sha, 0, 8);

    if ($is_noise) {
        $output .= colored("$sha8  $e->{date}  $e->{subject}  $e->{author}", 'bright_black') . "\n";
    } else {
        $output .= colored($sha8, 'cyan') . '  ';
        $output .= colored($e->{date}, 'blue') . '  ';
        $output .= $e->{subject} . '  ';
        $output .= colored($e->{author}, 'bright_black') . "\n";

        if ($diff) {
            for my $dline (split /\n/, $diff) {
                if ($dline =~ /^diff --git/) {
                    $output .= colored($dline, 'magenta') . "\n";
                } elsif ($dline =~ /^--- /) {
                    $output .= colored($dline, 'red') . "\n";
                } elsif ($dline =~ /^\+\+\+ /) {
                    $output .= colored($dline, 'green') . "\n";
                } elsif ($dline =~ /^@@ /) {
                    $output .= colored($dline, 'cyan') . "\n";
                } elsif ($dline =~ /^\+/) {
                    $output .= colored($dline, 'green') . "\n";
                } elsif ($dline =~ /^-/) {
                    $output .= colored($dline, 'red') . "\n";
                } else {
                    $output .= "$dline\n";
                }
            }
        }
        $output .= "\n";
    }
}

# Pipe through less -R for paging with color support
open my $less, '|-', 'less', '-R' or die "Cannot run less: $!";
print $less $output;
close $less;

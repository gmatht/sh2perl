#!/usr/bin/env perl
use strict;
use warnings;
use Term::ANSIColor qw(color colored);

# Colored git log.  Commits whose diff is ONLY numeric/ID changes
# are shown dimmed.  Everything else is normal.

my @git_args = @ARGV ? @ARGV : ('-20');

my $format = '%H|%ad|%s|%an';
my $datefmt = '--date=format:%Y-%m-%d %H:%M';

open my $fh, '-|', 'git', 'log', @git_args, "--format=format:$format", $datefmt
    or die "Cannot run git log: $!";

my @entries;
while (<$fh>) {
    chomp;
    my ($sha, $date, $subject, $author) = split /\|/, $_, 4;
    push @entries, { sha => $sha, date => $date, subject => $subject, author => $author };
}
close $fh;

for my $e (@entries) {
    my $diff = `git diff ${$e}{sha}^..${$e}{sha} 2>/dev/null`;
    my $is_noise = 1;

    if ($diff) {
        for my $dline (split /\n/, $diff) {
            next if $dline =~ /^diff --git|^index |^--- |^\+\+\+ |^@@ /;
            next if $dline =~ /^[ -]$/;
            next if $dline =~ /^[ ]/;  # context
            next if $dline =~ /^[+-]\s*$/;

            my $c = $dline;
            $c =~ s/^[+-]\s*//;

            # Pure number changes (counter increments)
            next if $c =~ /^\d+$/;
            # Variable-like IDs: _14 → _200
            next if $c =~ /^[a-zA-Z_]\w*_\d+$/;
            # File mode changes
            next if $c =~ /^(new|deleted) file mode/;
            next if $c =~ /^Binary files/;
            # Empty content
            next if $c eq '';

            $is_noise = 0;
            last;
        }
    }

    my $sha_str = substr(${$e}{sha}, 0, 8);
    my $line = sprintf "%-8s %s  %s  %s",
        $sha_str, ${$e}{date}, ${$e}{subject}, ${$e}{author};

    if ($is_noise) {
        print colored($line, 'bright_black'), "\n";
    } else {
        print colored($sha_str, 'cyan'), '  ';
        print colored(${$e}{date}, 'blue'), '  ';
        print ${$e}{subject}, '  ';
        print colored(${$e}{author}, 'bright_black'), "\n";
    }
}

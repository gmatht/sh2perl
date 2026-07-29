#!/usr/bin/env perl
use strict;
use warnings;
use Term::ANSIColor qw(colored);

# Colored git log.  Commits whose diff is ONLY numeric/ID changes
# are shown dimmed.  Everything else shows its patch (-p style).
#
# Usage: perl meangitlog.pl [git-log-options]
#   Default: -20, showing patches for meaningful commits.

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
    my $sha = $e->{sha};
    my $diff = `git diff '$sha^'..'$sha' 2>/dev/null`;
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
        print colored("$sha8  $e->{date}  $e->{subject}  $e->{author}", 'bright_black'), "\n";
    } else {
        print colored($sha8, 'cyan'), '  ';
        print colored($e->{date}, 'blue'), '  ';
        print $e->{subject}, '  ';
        print colored($e->{author}, 'bright_black'), "\n";

        # Show the diff/patch for meaningful commits
        if ($diff) {
            for my $dline (split /\n/, $diff) {
                if ($dline =~ /^diff --git/) {
                    print colored($dline, 'magenta'), "\n";
                } elsif ($dline =~ /^--- /) {
                    print colored($dline, 'red'), "\n";
                } elsif ($dline =~ /^\+\+\+ /) {
                    print colored($dline, 'green'), "\n";
                } elsif ($dline =~ /^@@ /) {
                    print colored($dline, 'cyan'), "\n";
                } elsif ($dline =~ /^\+/) {
                    print colored($dline, 'green'), "\n";
                } elsif ($dline =~ /^-/) {
                    print colored($dline, 'red'), "\n";
                } else {
                    print "$dline\n";
                }
            }
        }
        print "\n";
    }
}

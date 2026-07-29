#!/usr/bin/env perl
use strict;
use warnings;
use Term::ANSIColor qw(colored);

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
    # Use quoted revision range so shell doesn't interpret ^
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
    my $msg  = "$sha8  $e->{date}  $e->{subject}  $e->{author}";

    if ($is_noise) {
        print colored($msg, 'bright_black'), "\n";
    } else {
        print colored($sha8, 'cyan'), '  ';
        print colored($e->{date}, 'blue'), '  ';
        print $e->{subject}, '  ';
        print colored($e->{author}, 'bright_black'), "\n";
    }
}

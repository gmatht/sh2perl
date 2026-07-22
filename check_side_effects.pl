#!/usr/bin/env perl
use strict;
use warnings;
use File::Find;
use Digest::MD5 qw(md5_hex);
use File::Temp qw(tempdir);
use Cwd qw(abs_path);

# check_side_effects.pl — compare file-system side effects of bash vs Perl
# Usage: check_side_effects.pl <test.sh>

my $WATCH_DIRS = ['.'];  # watch sh2perl/ (examples, examples.out, etc.)
my $debashc = './target/debug/debashc';
my $violations = 0;

$SIG{__WARN__} = sub {};

sub snapshot {
    my ($dirs) = @_;
    my %state;
    for my $dir (@$dirs) {
        next unless -d $dir;
        find({
            wanted => sub {
                return if -d $_;
                my $path = $File::Find::name;
                return unless -f $path && -r _;
                my ($size) = (stat(_))[7];
                open my $fh, '<', $path or return;
                my $md5 = md5_hex(do { local $/; <$fh> });
                close $fh;
                $state{$path} = { md5 => $md5, size => $size };
            },
            no_chdir => 1,
        }, $dir);
    }
    return \%state;
}

sub diff_snapshot {
    my ($before, $after) = @_;
    my @changes;
    for my $path (keys %$after) {
        if (!exists $before->{$path}) {
            push @changes, "CREATED $path";
        } elsif ($before->{$path}{md5} ne $after->{$path}{md5}) {
            push @changes, "MODIFIED $path";
        }
    }
    for my $path (keys %$before) {
        if (!exists $after->{$path}) {
            push @changes, "DELETED $path";
        }
    }
    return @changes;
}

sub capture_side_effects {
    my ($script, $mode) = @_;
    my $before = snapshot($WATCH_DIRS);

    if ($mode eq 'bash') {
        system('bash', $script);
    } else {
        my $raw = `$debashc parse --perl "$script" 2>/dev/null`;
        if ($? != 0 || !defined $raw || $raw !~ m{#!/}) {
            return ("PERL GENERATION FAILED");
        }
        # Strip everything before the first #!
        my ($perl_code) = $raw =~ /(^#!.*?)^===/ms;
        return ("NO PERL CODE") unless defined $perl_code && length $perl_code > 10;

        my $tmp_dir = tempdir(CLEANUP => 1);
        my $pl_file = "$tmp_dir/check_side.pl";
        open my $fh, '>', $pl_file or return ("Cannot write $pl_file: $!");
        print $fh $perl_code;
        close $fh;
        system('perl', $pl_file);
    }

    my $after = snapshot($WATCH_DIRS);
    my @changes = diff_snapshot($before, $after);
    # Remove expected changes (test runner artifacts, generated .pl files)
    @changes = grep {
        !m{/check_side\.pl$} &&
        !m{/failing_tests\.txt$} &&
        !m{/\.last_trusted_count$} &&
        !m{/first_n_tests_passed\.txt$} &&
        !m{/command_cache\.json$} &&
        !m{/fix_history\.log$} &&
        !m{/crash_output\.log$} &&
        !m{/debug_perl_code\.pl$} &&
        !m{/examples\.out/.*\.pl$}
    } @changes;
    return @changes;
}

# Main
my $script = shift @ARGV;
die "Usage: $0 <test.sh>\n" unless $script && -f $script;
$script = abs_path($script);

print "=== Side-effect check: $script ===\n\n";

print "--- [bash] ---\n";
my @bash_changes = capture_side_effects($script, 'bash');
print "Bash: " . scalar(@bash_changes) . " side effect(s)\n";
print "  $_\n" for @bash_changes;
print "\n";

print "--- [perl] ---\n";
my @perl_changes = capture_side_effects($script, 'perl');
print "Perl: " . scalar(@perl_changes) . " side effect(s)\n";
print "  $_\n" for @perl_changes;
print "\n";

print "--- Comparison ---\n";
my %bash_set = map { $_ => 1 } @bash_changes;
my %perl_set = map { $_ => 1 } @perl_changes;
my $bash_only = 0;
my $perl_only = 0;

for my $c (@bash_changes) {
    unless ($perl_set{$c}) {
        print "BASH ONLY: $c\n";
        $bash_only++;
    }
}
for my $c (@perl_changes) {
    unless ($bash_set{$c}) {
        print "PERL ONLY: $c\n";
        $perl_only++;
    }
}

if ($bash_only == 0 && $perl_only == 0) {
    print "OK: side effects match\n";
} else {
    print "FAIL: side effects differ ($bash_only bash-only, $perl_only perl-only)\n";
    $violations++;
}

exit $violations;

#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;

my $main_exit_code = 0;

print "Testing system calls with builtin commands\n";
my $result1 = do {
    my $cmd_result_1 = do {
    my @ls_files_0 = ();
    if (-f q{.}) {
        push @ls_files_0, q{.};
    } elsif (-d q{.}) {
        if (opendir my $dh, q{.}) {
            while (my $file = readdir $dh) {
                push @ls_files_0, $file;
            }
            closedir $dh;
            @ls_files_0 = sort { $a cmp $b } @ls_files_0;
        }
    }
    join "\n", @ls_files_0;
};
    chomp $cmd_result_1;
    $cmd_result_1;
};
my $result2 = do {
    my $cmd_result_2 = do {
    my @results;
    my $start_path = q{.};
    sub find_files {
        my ($dir) = @_;
        if (opendir my $dh, $dir) {
            while (my $file = readdir $dh) {
                next if $file eq q{.} or $file eq q{..};
                my $full_path = "$dir/$file";
                if (-d $full_path) {
                    find_files($full_path);
                } else {
                    {
                        if ($file =~ /.*[.]txt$/msx) {
                            push @results, $full_path;
                        }
                    }
                }
            }
        closedir $dh;
    }
    return;
}
find_files($start_path);
join "\n", @results;
};
    chomp $cmd_result_2;
    $cmd_result_2;
};
print "Results:\n";
print $result1, "\n";
print $result2, "\n";

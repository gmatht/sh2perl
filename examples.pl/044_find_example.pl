#!/usr/bin/env perl

use strict;
use warnings;
use File::Find;

# Find all .txt files in current directory and subdirectories
print '#find . -name "*.txt" -type f | sort' . "\n";
my @txt_files;
find(sub { push @txt_files, $File::Find::name if /\.txt$/ && -f }, '.');
@txt_files = sort @txt_files;
print join("\n", @txt_files) . "\n";

# Find files modified in the last 7 days
print "\nfind . -mtime -7 -type f  | sort\n";
my @recent_files;
my $seven_days_ago = time - (7 * 24 * 60 * 60);
find(sub { 
    push @recent_files, $File::Find::name if -f && (stat($_))[9] > $seven_days_ago;
}, '.');
@recent_files = sort @recent_files;
print join("\n", @recent_files) . "\n";

# Find files modified in the last 1 day
print "\nfind . -mtime -1 -type f  | sort\n";
@recent_files = ();
my $one_day_ago = time - (24 * 60 * 60);
find(sub { 
    push @recent_files, $File::Find::name if -f && (stat($_))[9] > $one_day_ago;
}, '.');
@recent_files = sort @recent_files;
print join("\n", @recent_files) . "\n";

# Find files modified in the last 1 hour
print "\nfind . -mmin -60 -type f  | sort\n";
@recent_files = ();
my $one_hour_ago = time - (60 * 60);
find(sub { 
    push @recent_files, $File::Find::name if -f && (stat($_))[9] > $one_hour_ago;
}, '.');
@recent_files = sort @recent_files;
print join("\n", @recent_files) . "\n";

# Find files larger than 1MB
print "\nfind . -size +1M -type f  | sort\n";
my @large_files;
find(sub { 
    push @large_files, $File::Find::name if -f && (stat($_))[7] > 1024*1024;
}, '.');
@large_files = sort @large_files;
print join("\n", @large_files) . "\n";

# Find empty files and directories
print "\nfind . -empty  | sort\n";
my @empty_items;
find(sub { 
    push @empty_items, $File::Find::name if -f && (stat($_))[7] == 0;
}, '.');
@empty_items = sort @empty_items;
print join("\n", @empty_items) . "\n";

# Find files and execute command on them
print "touch/ls/rm\n";
open(my $fh, '>', "a.logtmp") or die "Cannot create a.logtmp: $!\n";
close($fh);
open($fh, '>', "a.logtmp.sav") or die "Cannot create a.logtmp.sav: $!\n";
close($fh);

find(sub { 
    unlink($_) if /\.logtmp$/ && -f;
}, '.');

opendir(my $dh, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh)) {
    print "$file\n" if $file =~ /\.logtmp/;
}
closedir($dh);

unlink("a.logtmp.sav");

# Find files and show detailed information
print "find . -type f -ls  | sort\n";
my @all_files;
find(sub { 
    push @all_files, $File::Find::name if -f;
}, '.');
@all_files = sort @all_files;
for my $file (@all_files) {
    my @stat = stat($file);
    printf "%s %d %s %s %d %s %s\n", 
        $stat[2], $stat[1], $stat[4], $stat[5], $stat[7], 
        scalar(localtime($stat[9])), $file;
}

# Find files excluding certain directories
print "find .. -type f -not -path \"./.git/*\" -not -path \"./node_modules/*\"  | sort\n";
my @filtered_files;
find(sub { 
    my $path = $File::Find::name;
    unless ($path =~ /\.git\// || $path =~ /node_modules\//) {
        push @filtered_files, $path if -f;
    }
}, '..');
@filtered_files = sort @filtered_files;
print join("\n", @filtered_files) . "\n";


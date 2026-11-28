#!/usr/bin/env perl
use strict;
use warnings;
$0 = 'examples/000__04a_basic_command_substitution.sh';

use feature 'say';

sub __sh_str {
    my $__v = shift();
    return defined($__v) ? $__v : q{};
}

my $current_date;
my $current_dir;

print "=== Basic Command Substitution ===\n";
$? = 0 << 8;
say "Current date: " . (do { my $__sh_cap; $__sh_cap = (require POSIX, POSIX::strftime('%Y', localtime)); $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap });
say "Current directory: " . (do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $__sh_cap }) }); $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap });
$current_date = do { my $__sh_cap; $__sh_cap = (require POSIX, POSIX::strftime('%Y%m', localtime)); $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
$current_dir = do { my $__sh_cap; $__sh_cap = (do { require File::Basename; File::Basename::basename(do { my $__sh_cap; $__sh_cap = (require Cwd, Cwd::getcwd()); $? = 0 << 8; $__sh_cap }) }); $? = 0 << 8; $__sh_cap =~ s/\n+\z// if defined $__sh_cap; $__sh_cap };
say "Stored date: " . __sh_str(($current_date));
$? = 0 << 8;
say "Stored directory: " . __sh_str(($current_dir));
$? = 0 << 8;
print "=== Basic Command Substitution Complete ===\n";
$? = 0 << 8;
$? = 0 << 8;

exit $? >> 8;

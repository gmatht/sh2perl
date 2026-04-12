#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

my $i = 0;

my $MAX_LOOP_5 = 5;
my $MAGIC_10   = 10;

if ((-f"file.txt")) {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
for my $i ( 1 .. $MAX_LOOP_5 ) {
    do {
    my $output = "Number: $i";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
}
$i = 5;
while ( $i < $MAGIC_10 ) {
    do {
    my $output = "Counter: $i";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
    $i = $i + 1;
}

sub greet {
    my ($file) = @_;
    do {
    my $output = "Hello, $_[0]!";
    print $output;
    if ( !( $output =~ m{\n\z}msx ) ) {
        print "\n";
    }
};
    return;
}
greet("World");

exit $main_exit_code;

#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 1 variables: ["i"]
my $i = 0;

if (-f "file.txt") {
        print("File exists\n");
;
} else {
        print("File does not exist\n");
;
}
for $i (1..5) {
    print(("Number: " . $i) . "\n");
;
}
while ($i < 10) {
    print(("Counter: " . $i) . "\n");
    $i = do { my $v = eval { $i + 1 }; $@ ? '' : $v };
$ENV{i} = $i;
}
sub greet {
    print(("Hello, " . $1 . "!") . "\n");
}
greet("World");
#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = 'pid_tempfile.sh';
my $tmpf = "/tmp/${\$}.txt";
open my $fh, '>', "\"\${tmpf}\"" or die ""${tmpf}": $!\n";
print {$fh}("hello", "\n");
close $fh;
print do { my $cat_chunk = q{}; if ( open my $fh, '<', "${tmpf}" ) { local $INPUT_RECORD_SEPARATOR = undef; $cat_chunk = <$fh>; close $fh; } else { carp 'cat: ' . "${tmpf}" . ': ' . $OS_ERROR . "\n"; } $cat_chunk; };
if ( -e "${tmpf}" ) {
    if ( -d "${tmpf}" ) {
        croak "rm: ", "${tmpf}",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "${tmpf}" ) {
                    }
        else {
            croak "rm: cannot remove ", "${tmpf}",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "${tmpf}", ": No such file or directory\n";
}

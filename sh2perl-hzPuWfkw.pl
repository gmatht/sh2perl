#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

our $CHILD_ERROR;

my $TRANSCAT;

$__set_e = 1;
if (("$1" eq "remove" || "$1" eq "upgrade")) {
    $TRANSCAT = '/etc/sgml/transitional.cat';
if ((-f ${TRANSCAT})) {
        unlink(TRANSCAT);
        unlink(TRANSCAT);
        unlink('.old');
        $main_exit_code = system('update-catalog', '--update-super') >> 8;
    }
}
my $temp_content = '/usr/local/share/sgml/declaration
/usr/local/share/sgml/dtd
/usr/local/share/sgml/entities
/usr/local/share/sgml/misc
/usr/local/share/sgml/stylesheet
/usr/local/share/sgml
';
use File::Path qw(make_path);
if (!-d q{/tmp}) { make_path(q{/tmp}); }
open my $fh_1, '>', q{/tmp} . '/heredoc_temp' or croak "Cannot create temp file: $OS_ERROR\n";
print $fh_1 $temp_content;
close $fh_1 or croak "Close failed: $OS_ERROR\n";
open STDIN, '<', q{/tmp} . '/heredoc_temp' or croak "Cannot open temp file: $OS_ERROR\n";
do {
    local %ENV = %ENV;
    my $dir;
while ( my $L = <> ) {
    chomp $L;
    my @_fields = split /\s+/msx, $L;
    $dir = $_fields[0] // q{};
                do {
local *STDERR;
open STDERR, '>', '/dev/null' or croak "Cannot access file: $OS_ERROR\n";
rmdir ("$dir") or warn "rmdir failed: $OS_ERROR\n";
$CHILD_ERROR = 0;
        };
        if ($CHILD_ERROR != 0) {
            1;
        }
;
    }
    q{};
};
exit 0;

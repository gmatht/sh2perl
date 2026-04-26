BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/003__ls_basic.pl" }
print 'Working Directory:';
do {
    my $__PURIFY_TMP = do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use Cwd;
my $pwd = getcwd();
$pwd . "\n";

    };
    if (defined $__PURIFY_TMP && $__PURIFY_TMP ne q{}) {
        print $__PURIFY_TMP;
        if (!($__PURIFY_TMP =~ m{\n\z}msx)) { print "\n"; }
    }
};

print 'Files: ';
my $ls_output = do {
    my @ls_files_0 = ();
    if ( -f q{.} ) {
        push @ls_files_0, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
                push @ls_files_0, $file;
            }
            closedir $dh;
            @ls_files_0 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_0;
        }
    }
    (@ls_files_0 ? join("\n", @ls_files_0) . "\n" : q{});
}
;
print $ls_output;

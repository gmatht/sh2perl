print 'Working Directory:';
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use Cwd;
my $pwd = getcwd();
print "$pwd\n";

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

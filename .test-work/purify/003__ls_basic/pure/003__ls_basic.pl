sub __bt { my $s = join('', @_); wantarray ? (split /^/, $s, -1) : $s }
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/003__ls_basic.pl" }
print 'Working Directory:';
my $pid = fork;if (!defined $pid) { die "fork failed: " . $!; } elsif ($pid == 0) { exec('sh', '-c', 'pwd'); die "exec failed: " . $!; } else { waitpid($pid, 0); }

print 'Files: ';
my $ls_output = __bt(do {
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
);
print $ls_output;

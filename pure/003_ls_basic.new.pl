#!/usr/bin/perl


print "=== Example 003: Basic ls command ===\n";

print "Using backticks to call ls:\n";
my $ls_output = my @ls_files_0 = ();
if ( -f q{.} ) {
    push @ls_files_0, q{.};
}
elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_0, $file;
        }
        closedir $dh;
        @ls_files_0 = sort { lc $a cmp lc $b } @ls_files_0;
    }
}
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
        local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 1;
    $ls_success = 0;
};
print $ls_output;

print "\nls with specific directory:\n";
if (-d 'src') {
    my @ls_files_0 = ();
} else {
    print "src directory not found, listing current directory:\n";
    my @ls_files_0 = ();
}

if ( -f q{.} ) {
    push @ls_files_0, q{.};
}
elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_0, $file;
        }
        closedir $dh;
        @ls_files_0 = sort { lc $a cmp lc $b } @ls_files_0;
    }
}
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
        local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 1;
    $ls_success = 0;
};

print "\nls -p (directories with / suffix):\n";
my @ls_files_0 = ();

print "\nls -a (including hidden files):\n";
my $hidden_output = my @ls_files_0 = ();
if ( -f q{.} ) {
    push @ls_files_0, q{.};
}
elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
        while ( my $file = readdir $dh ) {
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_0, $file;
        }
        closedir $dh;
        @ls_files_0 = sort { lc $a cmp lc $b } @ls_files_0;
    }
}
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
        local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 1;
    $ls_success = 0;
};
print $hidden_output;

print "\nls -t (sorted by modification time):\n";
my @ls_files_0 = ();

print "\nls *.pl (Perl files only):\n";
my $perl_files = DEBUG: ls command file/directory argument: '*.pl'
open STDERR, '>', '/dev/null' or croak "Cannot open file: $OS_ERROR\n";
my @ls_files_0 = ();
if ( -f '*.pl' ) {
    push @ls_files_0, '*.pl';
}
elsif ( -d '*.pl' ) {
    if ( opendir my $dh, '*.pl' ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            next if $file =~ /^__tmp_.*[.]pl$/msx;
            next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
            push @ls_files_0, $file;
        }
        closedir $dh;
    }
}
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
    my $expected_count = 1;
    if ( @ls_files_0 == $expected_count ) {
        local $CHILD_ERROR = 0;
        $ls_success = 1;
    }
    else {
        local $CHILD_ERROR = 2;
        $ls_success = 0;
    }
}
else {
    local $CHILD_ERROR = 1;
    $ls_success = 0;
};
if ($perl_files) {
    print $perl_files;
} else {
    print "No .pl files found\n";
}

print "\nls multiple directories:\n";
my @ls_files_0 = ();

print "=== Example 003 completed successfully ===\n";

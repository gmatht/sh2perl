#!/usr/bin/perl


print "=== Example 003: Basic ls command ===\n";

print "Using backticks to call ls:\n";
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

print "\nls with specific directory:\n";
if (-d 'src') {
    do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my @ls_files_0 = ();
my $ls_all_found_1 = 1;
my @ls_inputs_2 = ();
push @ls_inputs_2, 'src';
my @ls_files_3 = ();
my @ls_dirs_4 = ();
my $ls_show_headers_5 = scalar(@ls_inputs_2) > 1;
for my $ls_item_6 (@ls_inputs_2) {
    if ( -f $ls_item_6 ) {
        push @ls_files_3, $ls_item_6;
    }
    elsif ( -d $ls_item_6 ) {
        push @ls_dirs_4, $ls_item_6;
    }
    else {
        $ls_all_found_1 = 0;
    }
}
@ls_files_3 = sort { $a cmp $b } @ls_files_3;
@ls_dirs_4 = sort { $a cmp $b } @ls_dirs_4;
if (@ls_files_3) {
    push @ls_files_0, join("\n", @ls_files_3);
}
for my $ls_dir_7 (@ls_dirs_4) {
    my @ls_dir_entries_8 = ();
    if ( opendir my $dh, $ls_dir_7 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_8, $file;
        }
        closedir $dh;
        @ls_dir_entries_8 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_8;
        if ( $ls_show_headers_5 ) {
            if ( @ls_dir_entries_8 ) {
                push @ls_files_0, $ls_dir_7 . ":\n" . join("\n", @ls_dir_entries_8);
            } else {
                push @ls_files_0, $ls_dir_7 . ':';
            }
        }
        elsif ( @ls_dir_entries_8 ) {
            push @ls_files_0, join("\n", @ls_dir_entries_8);
        }
    }
    else {
        $ls_all_found_1 = 0;
    }
}
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
}
if ( $ls_all_found_1 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}

};
} else {
    print "src directory not found, listing current directory:\n";
    do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
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
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
}
local $CHILD_ERROR = 0;
$ls_success = 1;

};
}


print "\nls -p (directories with / suffix):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my @ls_files_0 = ();
if ( -f q{.} ) {
    push @ls_files_0, q{.};
}
elsif ( -d q{.} ) {
    if ( opendir my $dh, q{.} ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            if ( -d "./$file" ) {
                push @ls_files_0, "$file/";
            } else {
                push @ls_files_0, $file;
            }
        }
        closedir $dh;
        @ls_files_0 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_0;
    }
}
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
}
local $CHILD_ERROR = 0;
$ls_success = 1;

};

print "\nls -a (including hidden files):\n";
my $hidden_output = do {
    my @ls_files_0 = ();
    if ( -f q{.} ) {
        push @ls_files_0, q{.};
    }
    elsif ( -d q{.} ) {
        if ( opendir my $dh, q{.} ) {
            while ( my $file = readdir $dh ) {
                push @ls_files_0, $file;
            }
            closedir $dh;
            @ls_files_0 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_files_0;
        }
    }
    (@ls_files_0 ? join("\n", @ls_files_0) . "\n" : q{});
}
;
print $hidden_output;

print "\nls -t (sorted by modification time):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
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
        use Time::HiRes qw(stat);
@ls_files_0 = sort { my $mtime_a = (stat("./$a"))[9]; my $mtime_b = (stat("./$b"))[9]; $mtime_b <=> $mtime_a || $a cmp $b } @ls_files_0;
    }
}
if (@ls_files_0) {
    print join "\n", @ls_files_0;
    print "\n";
}
local $CHILD_ERROR = 0;
$ls_success = 1;

};

print "\nls *.pl (Perl files only):\n";
my $perl_files = do { my $command = 'ls *.pl 2> /dev/null'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; }
;
if ($perl_files) {
    print $perl_files;
} else {
    print "No .pl files found\n";
}

print "\nls multiple directories:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my @ls_files_0 = ();
my $ls_all_found_1 = 1;
my @ls_inputs_2 = ();
push @ls_inputs_2, q{.};
push @ls_inputs_2, 'src';
my @ls_files_3 = ();
my @ls_dirs_4 = ();
my $ls_show_headers_5 = scalar(@ls_inputs_2) > 1;
for my $ls_item_6 (@ls_inputs_2) {
    if ( -f $ls_item_6 ) {
        push @ls_files_3, $ls_item_6;
    }
    elsif ( -d $ls_item_6 ) {
        push @ls_dirs_4, $ls_item_6;
    }
    else {
        $ls_all_found_1 = 0;
    }
}
@ls_files_3 = sort { $a cmp $b } @ls_files_3;
@ls_dirs_4 = sort { $a cmp $b } @ls_dirs_4;
if (@ls_files_3) {
    push @ls_files_0, join("\n", @ls_files_3);
}
for my $ls_dir_7 (@ls_dirs_4) {
    my @ls_dir_entries_8 = ();
    if ( opendir my $dh, $ls_dir_7 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_8, $file;
        }
        closedir $dh;
        @ls_dir_entries_8 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_8;
        if ( $ls_show_headers_5 ) {
            if ( @ls_dir_entries_8 ) {
                push @ls_files_0, $ls_dir_7 . ":\n" . join("\n", @ls_dir_entries_8);
            } else {
                push @ls_files_0, $ls_dir_7 . ':';
            }
        }
        elsif ( @ls_dir_entries_8 ) {
            push @ls_files_0, join("\n", @ls_dir_entries_8);
        }
    }
    else {
        $ls_all_found_1 = 0;
    }
}
if (@ls_files_0) {
    print join "\n\n", @ls_files_0;
    print "\n";
}
if ( $ls_all_found_1 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}

};

print "=== Example 003 completed successfully ===\n";

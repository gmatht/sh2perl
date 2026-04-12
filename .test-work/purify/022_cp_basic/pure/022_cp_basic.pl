#!/usr/bin/perl


print "=== Example 022: Basic cp command ===\n";

open(my $fh, '>', 'test_cp_source.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for copying\n";
print $fh "It has multiple lines\n";
print $fh "To demonstrate cp functionality\n";
close($fh);

do {
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Path qw(make_path);
my $err;
if ( !-d 'test_cp_dir' ) {
    make_path( 'test_cp_dir', { error => \$err } );
    if ( @{$err} ) {
        croak "mkdir: cannot create directory " . 'test_cp_dir' . ": $err->[0]\n";
    }
}

};

print "Using " . "sys" . "tem" . "() to call cp (copy file):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
do {
    my $cp_cmd = 'cp test_cp_source.txt test_cp_dest.txt';
    my $cp_output = qx{$cp_cmd};
    print $cp_output;
    $cp_output;
};

};
if (-f "test_cp_dest.txt") {
    print "File copied successfully\n";
    my $content = do { open my $fh, '<', 'test_cp_dest.txt' or die 'cat: ' . 'test_cp_dest.txt' . ': ' . $OS_ERROR . "\n"; local $INPUT_RECORD_SEPARATOR = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $OS_ERROR . "\n"; $chunk; }
;
    print "Content: $content";
}

print "\ncp with recursive (-r):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
do {
    my $cp_cmd = 'cp -r test_cp_dir test_cp_dir_copy';
    my $cp_output = qx{$cp_cmd};
    print $cp_output;
    $cp_output;
};

};
if (-d "test_cp_dir_copy") {
    print "Directory copied successfully\n";
}

print "\ncp with preserve (-p):\n";
my $cp_preserve = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -p test_cp_source.txt test_cp_preserve.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_preserve.txt") {
    print "File copied with preserve attributes\n";
}

print "\ncp with verbose (-v):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
do {
    my $cp_cmd = 'cp -v test_cp_source.txt test_cp_verbose.txt';
    my $cp_output = qx{$cp_cmd};
    print $cp_output;
    $cp_output;
};

};

print "\ncp with force (-f):\n";
my $cp_force = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -f test_cp_source.txt test_cp_force.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_force.txt") {
    print "File copied with force\n";
}

print "\ncp with interactive (-i):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
do {
    my $cp_cmd = 'cp -i test_cp_source.txt test_cp_interactive.txt';
    my $cp_output = qx{$cp_cmd};
    print $cp_output;
    $cp_output;
};

};

print "\ncp with update (-u):\n";
my $cp_update = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -u test_cp_source.txt test_cp_update.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_update.txt") {
    print "File copied with update\n";
}

print "\ncp with backup (-b):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
do {
    my $cp_cmd = 'cp -b test_cp_source.txt test_cp_backup.txt';
    my $cp_output = qx{$cp_cmd};
    print $cp_output;
    $cp_output;
};

};

print "\ncp with suffix (--suffix=.bak):\n";
my $cp_suffix = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp --suffix=.bak test_cp_source.txt test_cp_suffix.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_suffix.txt") {
    print "File copied with suffix\n";
}

print "\ncp with multiple files:\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
do {
    my $cp_cmd = 'cp test_cp_source.txt test_cp_source2.txt test_cp_dir/';
    my $cp_output = qx{$cp_cmd};
    print $cp_output;
    $cp_output;
};

};

print "\ncp with preserve all (-a):\n";
my $cp_all = do {
        local $CHILD_ERROR = 0;
        my $eval_result = eval {
            do {
                my $cp_cmd = 'cp -a test_cp_source.txt test_cp_all.txt';
                my $cp_output = qx{$cp_cmd};
                $cp_output;
            };
            local $CHILD_ERROR = 0;
            1;
        };
        if ( !$eval_result ) {
            local $CHILD_ERROR = 256;
        }
        q{};
}
;
if (-f "test_cp_all.txt") {
    print "File copied with preserve all\n";
}

print "\ncp with no dereference (-P):\n";
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
use File::Copy qw(copy move);
do {
    my $cp_cmd = 'cp -P test_cp_source.txt test_cp_no_deref.txt';
    my $cp_output = qx{$cp_cmd};
    print $cp_output;
    $cp_output;
};

};

unlink('test_cp_source.txt') if -f 'test_cp_source.txt';
unlink('test_cp_dest.txt') if -f 'test_cp_dest.txt';
unlink('test_cp_preserve.txt') if -f 'test_cp_preserve.txt';
unlink('test_cp_verbose.txt') if -f 'test_cp_verbose.txt';
unlink('test_cp_force.txt') if -f 'test_cp_force.txt';
unlink('test_cp_interactive.txt') if -f 'test_cp_interactive.txt';
unlink('test_cp_update.txt') if -f 'test_cp_update.txt';
unlink('test_cp_backup.txt') if -f 'test_cp_backup.txt';
unlink('test_cp_suffix.txt') if -f 'test_cp_suffix.txt';
unlink('test_cp_source2.txt') if -f 'test_cp_source2.txt';
unlink('test_cp_all.txt') if -f 'test_cp_all.txt';
unlink('test_cp_no_deref.txt') if -f 'test_cp_no_deref.txt';
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_cp_dir" ) {
    if ( -d "test_cp_dir" ) {
        my $err;
        remove_tree("test_cp_dir", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_cp_dir", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_cp_dir" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_cp_dir",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};
do {
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
if ( -e "test_cp_dir_copy" ) {
    if ( -d "test_cp_dir_copy" ) {
        my $err;
        remove_tree("test_cp_dir_copy", {error => \$err});
        if (@{$err}) {
            carp "rm: carping: could not remove ", "test_cp_dir_copy", ": $err->[0]\n";
        }
        else {
            $main_exit_code = 0;
        }
    }
    else {
        if ( unlink "test_cp_dir_copy" ) {
            $main_exit_code = 0;
        }
        else {
            carp "rm: carping: could not remove ", "test_cp_dir_copy",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 0;
}

};

print "=== Example 022 completed successfully ===\n";

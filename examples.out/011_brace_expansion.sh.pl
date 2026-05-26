#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== Basic brace expansion ==\n";
print "1 2 3 4 5\n";
print "a b c\n";
print "00 02 04\n";
print "== Advanced brace expansion ==\n";
my @combinations = (
    ['a', '1'],
    ['a', '2'],
    ['a', '3'],
    ['b', '1'],
    ['b', '2'],
    ['b', '3'],
    ['c', '1'],
    ['c', '2'],
    ['c', '3'],
);
my @all_combinations;
for my $combo (@combinations) {
push @all_combinations, join("", @$combo);
}
print join(" ", @all_combinations) . "\n";
$CHILD_ERROR = 0;
print "1 3 5 7 9\n";
print "a d g j m p s v y\n";
print "== Practical examples ==\n";
if ( -e "file_001.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "file_001.txt";
}
else {
    if ( open my $fh, '>', "file_001.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "file_001.txt",
          ": $ERRNO\n";
    }
}
if ( -e "file_002.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "file_002.txt";
}
else {
    if ( open my $fh, '>', "file_002.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "file_002.txt",
          ": $ERRNO\n";
    }
}
if ( -e "file_003.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "file_003.txt";
}
else {
    if ( open my $fh, '>', "file_003.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "file_003.txt",
          ": $ERRNO\n";
    }
}
if ( -e "file_004.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "file_004.txt";
}
else {
    if ( open my $fh, '>', "file_004.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "file_004.txt",
          ": $ERRNO\n";
    }
}
if ( -e "file_005.txt" ) {
    my $current_time = time;
    utime $current_time, $current_time, "file_005.txt";
}
else {
    if ( open my $fh, '>', "file_005.txt" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "file_005.txt",
          ": $ERRNO\n";
    }
}
my @ls_files_1 = ();
my $ls_all_found_2 = 1;
my @ls_inputs_3 = ();
my @ls_glob_ls_inputs_3_0 = glob('file_*.txt');
if ( !@ls_glob_ls_inputs_3_0 ) {
    push @ls_inputs_3, 'file_*.txt';
    $ls_all_found_2 = 0;
} else {
    push @ls_inputs_3, @ls_glob_ls_inputs_3_0;
}
my @ls_files_4 = ();
my @ls_dirs_5 = ();
my $ls_show_headers_6 = scalar(@ls_inputs_3) > 1;
for my $ls_item_7 (@ls_inputs_3) {
    if ( -f $ls_item_7 ) {
        push @ls_files_4, $ls_item_7;
    }
    elsif ( -d $ls_item_7 ) {
        push @ls_dirs_5, $ls_item_7;
    }
    else {
        $ls_all_found_2 = 0;
    }
}
@ls_files_4 = sort { $a cmp $b } @ls_files_4;
@ls_dirs_5 = sort { $a cmp $b } @ls_dirs_5;
if (@ls_files_4) {
    push @ls_files_1, join("\n", @ls_files_4);
}
for my $ls_dir_8 (@ls_dirs_5) {
    my @ls_dir_entries_9 = ();
    if ( opendir my $dh, $ls_dir_8 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_9, $file;
        }
        closedir $dh;
        @ls_dir_entries_9 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_9;
        if ( $ls_show_headers_6 ) {
            if ( @ls_dir_entries_9 ) {
                push @ls_files_1, $ls_dir_8 . ":\n" . join("\n", @ls_dir_entries_9);
            } else {
                push @ls_files_1, $ls_dir_8 . ':';
            }
        }
        elsif ( @ls_dir_entries_9 ) {
            push @ls_files_1, join("\n", @ls_dir_entries_9);
        }
    }
    else {
        $ls_all_found_2 = 0;
    }
}
if (@ls_files_1) {
    print join "\n", @ls_files_1;
    print "\n";
}
if ( $ls_all_found_2 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}
my @files_to_remove = glob("file_*.txt");
foreach my $file_to_remove (@files_to_remove) {
    if ( -e $file_to_remove ) {
        if ( -d $file_to_remove ) {
            croak "rm: ", $file_to_remove,
    " is a directory (use -r to remove recursively)\n";
        }
        else {
            if ( unlink $file_to_remove ) {
            }
            else {
                local $CHILD_ERROR = 1;
                croak "rm: cannot remove ", $file_to_remove,
    ": $OS_ERROR\n";
            }
        }
    }
    else {
        local $CHILD_ERROR = 1;
        croak "rm: ", $file_to_remove,
    ": No such file or directory\n";
    }
}

exit $main_exit_code;

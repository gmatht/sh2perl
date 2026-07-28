#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $ls_success     = 0;
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '011_brace_expansion.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== Basic brace expansion ==";
say "1 2 3 4 5";
say "a b c";
say "00 02 04";
say "== Advanced brace expansion ==";
print join(q[ ], ('a' . '1', 'a' . '2', 'a' . '3', 'b' . '1', 'b' . '2', 'b' . '3', 'c' . '1', 'c' . '2', 'c' . '3')) . "\n";
say "1 3 5 7 9";
say "a d g j m p s v y";
say "== Practical examples ==";
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
my @ls_files_144 = ();
my $ls_all_found_145 = 1;
my @ls_inputs_146 = ();
my @ls_glob_ls_inputs_146_0 = glob('file_*.txt');
if ( !@ls_glob_ls_inputs_146_0 ) {
    push @ls_inputs_146, 'file_*.txt';
    $ls_all_found_145 = 0;
} else {
    push @ls_inputs_146, @ls_glob_ls_inputs_146_0;
}
my @ls_files_147 = ();
my @ls_dirs_148 = ();
my $ls_show_headers_149 = scalar(@ls_inputs_146) > 1;
for my $ls_item_150 (@ls_inputs_146) {
    if ( -f $ls_item_150 ) {
        push @ls_files_147, $ls_item_150;
    }
    elsif ( -d $ls_item_150 ) {
        push @ls_dirs_148, $ls_item_150;
    }
    else {
        $ls_all_found_145 = 0;
    }
}
@ls_files_147 = sort { $a cmp $b } @ls_files_147;
@ls_dirs_148 = sort { $a cmp $b } @ls_dirs_148;
if (@ls_files_147) {
    push @ls_files_144, join("\n", @ls_files_147);
}
for my $ls_dir_151 (@ls_dirs_148) {
    my @ls_dir_entries_152 = ();
    if ( opendir my $dh, $ls_dir_151 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/;
            push @ls_dir_entries_152, $file;
        }
        closedir $dh;
        @ls_dir_entries_152 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}; $s } ] } @ls_dir_entries_152;
        if ( $ls_show_headers_149 ) {
            if ( @ls_dir_entries_152 ) {
                push @ls_files_144, $ls_dir_151 . ":\n" . join("\n", @ls_dir_entries_152);
            } else {
                push @ls_files_144, $ls_dir_151 . ':';
            }
        }
        elsif ( @ls_dir_entries_152 ) {
            push @ls_files_144, join("\n", @ls_dir_entries_152);
        }
    }
    else {
        $ls_all_found_145 = 0;
    }
}
if (@ls_files_144) {
    print join "\n", @ls_files_144;
    print "\n";
}
if ( $ls_all_found_145 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
    $main_exit_code = $CHILD_ERROR;
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

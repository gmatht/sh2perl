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
my $__set_e        = 0;
our $CHILD_ERROR;

$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
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
my @ls_files_229 = ();
my $ls_all_found_230 = 1;
my @ls_inputs_231 = ();
my @ls_glob_ls_inputs_231_0 = glob('file_*.txt');
if ( !@ls_glob_ls_inputs_231_0 ) {
    push @ls_inputs_231, 'file_*.txt';
    $ls_all_found_230 = 0;
} else {
    push @ls_inputs_231, @ls_glob_ls_inputs_231_0;
}
my @ls_files_232 = ();
my @ls_dirs_233 = ();
my $ls_show_headers_234 = scalar(@ls_inputs_231) > 1;
for my $ls_item_235 (@ls_inputs_231) {
    if ( -f $ls_item_235 ) {
        push @ls_files_232, $ls_item_235;
    }
    elsif ( -d $ls_item_235 ) {
        push @ls_dirs_233, $ls_item_235;
    }
    else {
        $ls_all_found_230 = 0;
    }
}
@ls_files_232 = sort { $a cmp $b } @ls_files_232;
@ls_dirs_233 = sort { $a cmp $b } @ls_dirs_233;
if (@ls_files_232) {
    push @ls_files_229, join("\n", @ls_files_232);
}
for my $ls_dir_236 (@ls_dirs_233) {
    my @ls_dir_entries_237 = ();
    if ( opendir my $dh, $ls_dir_236 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_237, $file;
        }
        closedir $dh;
        @ls_dir_entries_237 = map { $_->[0] } sort { $a->[1] cmp $b->[1] } map { [ $_, do { (my $s = $_) =~ s{/$}{}msx; $s } ] } @ls_dir_entries_237;
        if ( $ls_show_headers_234 ) {
            if ( @ls_dir_entries_237 ) {
                push @ls_files_229, $ls_dir_236 . ":\n" . join("\n", @ls_dir_entries_237);
            } else {
                push @ls_files_229, $ls_dir_236 . ':';
            }
        }
        elsif ( @ls_dir_entries_237 ) {
            push @ls_files_229, join("\n", @ls_dir_entries_237);
        }
    }
    else {
        $ls_all_found_230 = 0;
    }
}
if (@ls_files_229) {
    print join "\n", @ls_files_229;
    print "\n";
}
if ( $ls_all_found_230 ) {
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

#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
use POSIX qw(time);

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print '#find . -name "*.txt" -type f | sort' . "\n";
# Original bash: find . -name "*.txt" -type f | sort
{
    my $output_13;
    my $output_printed_13;
    my $pipeline_success_13 = 1;
        $output_13 = do {
    use File::Find;
    use File::Basename;
    my @files_14 = ();
    my $start_14 = q{.};
    sub find_files_14 {
    my $file_14 = $File::Find::name;
    if ( !( -f $file_14 ) ) {
    return;
    }
    push @files_14, $file_14;
    return;
    }
    find( \&find_files_14, $start_14 );
    join "\n", @files_14;
    };

        my @sort_lines_13_1 = split /\n/msx, $output_13;
    my @sort_sorted_13_1 = sort @sort_lines_13_1;
    my $output_13_1 = join "\n", @sort_sorted_13_1;
    if ($output_13_1 ne q{} && !($output_13_1 =~ m{\n\z}msx)) {
    $output_13_1 .= "\n";
    }
    $output_13 = $output_13_1;
    $output_13 = $output_13_1;
    if ($output_13 ne q{} && !defined $output_printed_13) {
        print $output_13;
        if (!($output_13 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_13 ) { $main_exit_code = 1; }
    }
print "\nfind . -mtime -7 -type f  | sort" . "\n";
# Original bash: find . -mtime -7 -type f  | sort
{
    my $output_15;
    my $output_printed_15;
    my $pipeline_success_15 = 1;
        $output_15 = do {
    use File::Find;
    use File::Basename;
    my @files_16 = ();
    my $start_16 = q{.};
    sub find_files_16 {
    my $file_16 = $File::Find::name;
    if ( !( -f $file_16 ) ) {
    return;
    }
    push @files_16, $file_16;
    return;
    }
    find( \&find_files_16, $start_16 );
    join "\n", @files_16;
    };

        my @sort_lines_15_1 = split /\n/msx, $output_15;
    my @sort_sorted_15_1 = sort @sort_lines_15_1;
    my $output_15_1 = join "\n", @sort_sorted_15_1;
    if ($output_15_1 ne q{} && !($output_15_1 =~ m{\n\z}msx)) {
    $output_15_1 .= "\n";
    }
    $output_15 = $output_15_1;
    $output_15 = $output_15_1;
    if ($output_15 ne q{} && !defined $output_printed_15) {
        print $output_15;
        if (!($output_15 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_15 ) { $main_exit_code = 1; }
    }
print "\nfind . -mtime -1 -type f  | sort" . "\n";
# Original bash: find . -mtime -1 -type f  | sort
{
    my $output_17;
    my $output_printed_17;
    my $pipeline_success_17 = 1;
        $output_17 = do {
    use File::Find;
    use File::Basename;
    my @files_18 = ();
    my $start_18 = q{.};
    sub find_files_18 {
    my $file_18 = $File::Find::name;
    if ( !( -f $file_18 ) ) {
    return;
    }
    push @files_18, $file_18;
    return;
    }
    find( \&find_files_18, $start_18 );
    join "\n", @files_18;
    };

        my @sort_lines_17_1 = split /\n/msx, $output_17;
    my @sort_sorted_17_1 = sort @sort_lines_17_1;
    my $output_17_1 = join "\n", @sort_sorted_17_1;
    if ($output_17_1 ne q{} && !($output_17_1 =~ m{\n\z}msx)) {
    $output_17_1 .= "\n";
    }
    $output_17 = $output_17_1;
    $output_17 = $output_17_1;
    if ($output_17 ne q{} && !defined $output_printed_17) {
        print $output_17;
        if (!($output_17 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_17 ) { $main_exit_code = 1; }
    }
print "\nfind . -mmin -60 -type f  | sort" . "\n";
# Original bash: find . -mmin -60 -type f  | sort
{
    my $output_19;
    my $output_printed_19;
    my $pipeline_success_19 = 1;
        $output_19 = do {
    use File::Find;
    use File::Basename;
    my @files_20 = ();
    my $start_20 = q{.};
    sub find_files_20 {
    my $file_20 = $File::Find::name;
    if ( !( -f $file_20 ) ) {
    return;
    }
    push @files_20, $file_20;
    return;
    }
    find( \&find_files_20, $start_20 );
    join "\n", @files_20;
    };

        my @sort_lines_19_1 = split /\n/msx, $output_19;
    my @sort_sorted_19_1 = sort @sort_lines_19_1;
    my $output_19_1 = join "\n", @sort_sorted_19_1;
    if ($output_19_1 ne q{} && !($output_19_1 =~ m{\n\z}msx)) {
    $output_19_1 .= "\n";
    }
    $output_19 = $output_19_1;
    $output_19 = $output_19_1;
    if ($output_19 ne q{} && !defined $output_printed_19) {
        print $output_19;
        if (!($output_19 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_19 ) { $main_exit_code = 1; }
    }
print "\nfind . -size +1M -type f  | sort" . "\n";
# Original bash: find . -size +1M -type f  | sort
{
    my $output_21;
    my $output_printed_21;
    my $pipeline_success_21 = 1;
        $output_21 = do {
    use File::Find;
    use File::Basename;
    my @files_22 = ();
    my $start_22 = q{.};
    sub find_files_22 {
    my $file_22 = $File::Find::name;
    if ( !( -f $file_22 ) ) {
    return;
    }
    push @files_22, $file_22;
    return;
    }
    find( \&find_files_22, $start_22 );
    join "\n", @files_22;
    };

        my @sort_lines_21_1 = split /\n/msx, $output_21;
    my @sort_sorted_21_1 = sort @sort_lines_21_1;
    my $output_21_1 = join "\n", @sort_sorted_21_1;
    if ($output_21_1 ne q{} && !($output_21_1 =~ m{\n\z}msx)) {
    $output_21_1 .= "\n";
    }
    $output_21 = $output_21_1;
    $output_21 = $output_21_1;
    if ($output_21 ne q{} && !defined $output_printed_21) {
        print $output_21;
        if (!($output_21 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_21 ) { $main_exit_code = 1; }
    }
print "\nfind . -empty  | sort" . "\n";
# Original bash: find . -empty  | sort
{
    my $output_23;
    my $output_printed_23;
    my $pipeline_success_23 = 1;
        $output_23 = do {
    use File::Find;
    use File::Basename;
    my @files_24 = ();
    my $start_24 = q{.};
    sub find_files_24 {
    my $file_24 = $File::Find::name;
    push @files_24, $file_24;
    return;
    }
    find( \&find_files_24, $start_24 );
    join "\n", @files_24;
    };

        my @sort_lines_23_1 = split /\n/msx, $output_23;
    my @sort_sorted_23_1 = sort @sort_lines_23_1;
    my $output_23_1 = join "\n", @sort_sorted_23_1;
    if ($output_23_1 ne q{} && !($output_23_1 =~ m{\n\z}msx)) {
    $output_23_1 .= "\n";
    }
    $output_23 = $output_23_1;
    $output_23 = $output_23_1;
    if ($output_23 ne q{} && !defined $output_printed_23) {
        print $output_23;
        if (!($output_23 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_23 ) { $main_exit_code = 1; }
    }
print 'touch/ls/rm' . "\n";
if ( -e "a.logtmp" ) {
    my $current_time = time;
    utime $current_time, $current_time, "a.logtmp";
}
else {
    if ( open my $fh, '>', "a.logtmp" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "a.logtmp",
          ": $ERRNO\n";
    }
}
if ( -e "a.logtmp.sav" ) {
    my $current_time = time;
    utime $current_time, $current_time, "a.logtmp.sav";
}
else {
    if ( open my $fh, '>', "a.logtmp.sav" ) {
        close $fh or croak "Close failed: $ERRNO";
    }
    else {
        croak "touch: cannot create ", "a.logtmp.sav",
          ": $ERRNO\n";
    }
}
do {
    use File::Find;
    use File::Basename;
    my @files_26 = ();
    my $start_26 = q{.};

    sub find_files_26 {
        my $file_26 = $File::Find::name;
        push @files_26, $file_26;
        return;
    }
    find( \&find_files_26, $start_26 );
    join "\n", @files_26;
}
my @ls_files_27 = ();
my $ls_all_found_28 = 1;
my @ls_inputs_29 = ();
my @ls_glob_ls_inputs_29_0 = glob('*.logtmp*');
if ( !@ls_glob_ls_inputs_29_0 ) {
    push @ls_inputs_29, '*.logtmp*';
    $ls_all_found_28 = 0;
} else {
    push @ls_inputs_29, @ls_glob_ls_inputs_29_0;
}
my @ls_files_30 = ();
my @ls_dirs_31 = ();
my $ls_show_headers_32 = scalar(@ls_inputs_29) > 1;
for my $ls_item_33 (@ls_inputs_29) {
    if ( -f $ls_item_33 ) {
        push @ls_files_30, $ls_item_33;
    }
    elsif ( -d $ls_item_33 ) {
        push @ls_dirs_31, $ls_item_33;
    }
    else {
        $ls_all_found_28 = 0;
    }
}
@ls_files_30 = sort { $a cmp $b } @ls_files_30;
@ls_dirs_31 = sort { $a cmp $b } @ls_dirs_31;
if (@ls_files_30) {
    push @ls_files_27, join("\n", @ls_files_30);
}
for my $ls_dir_34 (@ls_dirs_31) {
    my @ls_dir_entries_35 = ();
    if ( opendir my $dh, $ls_dir_34 ) {
        while ( my $file = readdir $dh ) {
            next if $file eq q{.} || $file eq q{..} || $file =~ /^[.]/msx;
            push @ls_dir_entries_35, $file;
        }
        closedir $dh;
        @ls_dir_entries_35 = sort { my $aa = $a; my $bb = $b; $aa =~ s{/$}{}; $bb =~ s{/$}{}; $aa cmp $bb } @ls_dir_entries_35;
        if ( $ls_show_headers_32 ) {
            if ( @ls_dir_entries_35 ) {
                push @ls_files_27, $ls_dir_34 . ":\n" . join("\n", @ls_dir_entries_35);
            } else {
                push @ls_files_27, $ls_dir_34 . ':';
            }
        }
        elsif ( @ls_dir_entries_35 ) {
            push @ls_files_27, join("\n", @ls_dir_entries_35);
        }
    }
    else {
        $ls_all_found_28 = 0;
    }
}
if (@ls_files_27) {
    print join "\n", @ls_files_27;
    print "\n";
}
if ( $ls_all_found_28 ) {
    local $CHILD_ERROR = 0;
    $ls_success = 1;
}
else {
    local $CHILD_ERROR = 2;
    $ls_success = 0;
}
if ( -e "a.logtmp.sav" ) {
    if ( -d "a.logtmp.sav" ) {
        croak "rm: ", "a.logtmp.sav",
          " is a directory (use -r to remove recursively)\n";
    }
    else {
        if ( unlink "a.logtmp.sav" ) {
            $main_exit_code = 0;
        }
        else {
            croak "rm: cannot remove ", "a.logtmp.sav",
              ": $OS_ERROR\n";
        }
    }
}
else {
    local $CHILD_ERROR = 1;
    croak "rm: ", "a.logtmp.sav", ": No such file or directory\n";
}
print 'find .. -type f -not -path "./.git/*" -not -path "./node_modules/*"  | sort' . "\n";
{
    my $output_36;
    my $output_printed_36;
    my $pipeline_success_36 = 1;
        $output_36 = do {
    use File::Find;
    use File::Basename;
    my @files_37 = ();
    my $start_37 = q{..};
    sub find_files_37 {
    my $file_37 = $File::Find::name;
    if ( !( -f $file_37 ) ) {
    return;
    }
    push @files_37, $file_37;
    return;
    }
    find( \&find_files_37, $start_37 );
    join "\n", @files_37;
    };

        my @sort_lines_36_1 = split /\n/msx, $output_36;
    my @sort_sorted_36_1 = sort @sort_lines_36_1;
    my $output_36_1 = join "\n", @sort_sorted_36_1;
    if ($output_36_1 ne q{} && !($output_36_1 =~ m{\n\z}msx)) {
    $output_36_1 .= "\n";
    }
    $output_36 = $output_36_1;
    $output_36 = $output_36_1;
    if ($output_36 ne q{} && !defined $output_printed_36) {
        print $output_36;
        if (!($output_36 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_36 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;

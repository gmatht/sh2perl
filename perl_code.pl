
> #!/usr/bin/env perl
  use strict;
  use warnings;
  use Carp;
  use English qw( -no_match_vars );
  use locale;
  use IPC::Open3;
  
  my $main_exit_code = 0;
  my $ls_success = 0;
  our $CHILD_ERROR;
  
  print "=== File and Directory Operations ===\n";
  my $file_list = do {
      my @ls_files_0 = ();
      if (-f q{.}) {
          push @ls_files_0, q{.};
      } elsif (-d q{.}) {
          if (opendir my $dh, q{.}) {
              while (my $file = readdir $dh) {
                  next if $file =~ /^__tmp_.*[.]pl$/msx;
                  next if $file =~ /^(debug_|temp_|test_|file\d*[.]txt)$/msx;
                  push @ls_files_0, $file;
              }
              closedir $dh;
              @ls_files_0 = sort { lc $a cmp lc $b } @ls_files_0;
          }
      }
      join "\n", @ls_files_0;
  }
  ;
  print "File listing:\n";
  print $file_list;
  if (!($file_list =~ /\n$/msx)) { print "\n"; }
  my $found_files = do {
      use File::Find;
      use File::Basename;
      my @files_1 = ();
      my $start_1 = q{.};
      sub find_files_1 {
          my $file_1 = $File::Find::name;
          if (!(-f $file_1)) {
              return;
          }
          if (!(basename($file_1) =~ m/^.*.sh$/xms)) {
              return;
          }
          push @files_1, $file_1;
          return;
      }
      find(\&find_files_1, $start_1);
      join "\n", @files_1;
  };
  print "Found shell scripts:\n";
  print $found_files;
  if (!($found_files =~ /\n$/msx)) { print "\n"; }
  
  
  --- Running generated Perl code ---
  Can't open perl script "__tmp_run.pl": No such file or directory
  Exit code: exit code: 2
  
  ==================================================
  TIMING COMPARISON
  ==================================================
  Perl execution time:  0.0643 seconds
  Bash execution time:  0.1473 seconds
  Perl is 2.29x faster than Bash
  
  ==================================================
  OUTPUT COMPARISON
  ==================================================
  Γ£ù DIFFERENCES FOUND:
  
  STDOUT DIFFERENCES:
  --- bash_stdout
  +++ perl_stdout
  -=== File and Directory Operations ===
  -File listing:
  -.
  -..
  -.cursorrules
  -.git
  -.test_purify.pl.swp
  -__tmp_run.pl
  -000__04_basic_backtick_usage.sh
  -000__04_basic_backtick_usage.sh.complex
  -000__04_basic_backtick_usage.sh.ORIGINAL
  -000__04_basic_backtick_usage_simple.sh
  -003
  -20
  -30
  -90
  -after_test.txt
  -bash_tests
  -benchmark.bat
  -benchmark.ps1
  -BENCHMARK_README.md
  -benchmark_system.pl
  -build-wasm.sh
  -Cargo.lock



> #!/usr/bin/env perl
  use strict;
  use warnings;
  use Carp;
  use English qw( -no_match_vars );
  use locale;
  use IPC::Open3;
  use File::Path qw(make_path remove_tree);
  
  my $main_exit_code = 0;
  my $ls_success = 0;
  our $CHILD_ERROR;
  
  my $MAGIC_3 = 3;
  my $MAGIC_5 = 5;
  
  print "=== Text Processing Commands ===\n";
  my $file_content = do {
      my $output_0;
      my $pipeline_success_0 = 1;
          $output_0 = q{};
      if (open my $fh, '<', "src/main.rs") {
      while (my $line = <$fh>) {
      $output_0 .= $line;
      }
      close $fh or croak "Close failed: $OS_ERROR";
      # Ensure content ends with newline to prevent line concatenation
          if (!($output_0 =~ /\n$/msx)) {
              $output_0 .= "\n";
          }
      } else {
      carp "cat: ", "src/main.rs", ": No such file or directory";
      $output_0 = q{};
      }
      my $num_lines = 5;
      my $head_line_count = 0;
      my $result = q{};
      my $input = $output_0;
      my $pos = 0;
      while ($pos < length $input && $head_line_count < $num_lines) {
          my $line_end = index $input, "\n", $pos;
          if ($line_end == -1) {
              $line_end = length $input;
          }
          my $head_line = substr $input, $pos, $line_end - $pos;
          $result .= $head_line . "\n";
          $pos = $line_end + 1;
          ++$head_line_count;
      }
      $output_0 = $result;
      if (!$pipeline_success_0) { $main_exit_code = 1; }
          $output_0;
  }
  ;
  print "First 5 lines of main.rs:\n";
  print $file_content;
  if (!($file_content =~ /\n$/msx)) { print "\n"; }
  my $grep_result = do { my $grep_result_1;
  my @grep_lines_1 = ();
  my @grep_filenames_1 = ();
  if (-f "src/main.rs") {
      open my $fh, '<', "src/main.rs" or croak "Cannot open file: $ERRNO";
      while (my $line = <$fh>) {
          chomp $line;
          push @grep_lines_1, $line;
          push @grep_filenames_1, "src/main.rs";
      }
      close $fh or croak "Close failed: $OS_ERROR";
  }
  my @grep_filtered_1 = grep { /fn/msx } @grep_lines_1;
  my @grep_numbered_1;
  for my $i (0..@grep_lines_1-1) {
      if (scalar grep { $_ eq $grep_lines_1[$i] } @grep_filtered_1) {
          push @grep_numbered_1, sprintf "%d:%s", $i + 1, $grep_lines_1[$i];
      }
  }
  $grep_result_1 = join "\n", @grep_numbered_1;
  $CHILD_ERROR = scalar @grep_filtered_1 > 0 ? 0 : 1;
   $grep_result_1; };
  print "Lines containing 'fn':\n";
  print $grep_result;
  if (!($grep_result =~ /\n$/msx)) { print "\n"; }
  my $sed_result = do {
      my $output_2;
      my $pipeline_success_2 = 1;
          $output_2 .= "Hello World\n";
      my @sed_lines_2 = split /\n/msx, $output_2;
      my @sed_result_2;
      foreach my $line (@sed_lines_2) {
      chomp $line;
      $line =~ s/World/Universe/gmsx;
      push @sed_result_2, $line;
      }
      $output_2 = join "\n", @sed_result_2;
      if (!$pipeline_success_2) { $main_exit_code = 1; }
          $output_2;
  }
  ;
  print "Sed result: $sed_result\n";
  my $awk_result = do {
      my $output_3;


use strict; use warnings; use Carp;
# create test_data.txt
open(my $fh, '>', 'test_data.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,Engineer,95.5\n";
print $fh "Bob,30,Manager,87.2\n";
print $fh "Charlie,35,Developer,92.8\n";
print $fh "Diana,28,Designer,88.9\n";
print $fh "Eve,32,Analyst,91.3\n";
print $fh "Frank,29,Engineer,89.7\n";
print $fh "Grace,31,Manager,93.1\n";
print $fh "Henry,27,Developer,86.4\n";
close($fh);

# pipeline1 block
my $output_0 = q{};
my $pipeline_success_0 = 1;
$output_0 = do { open my $fh, '<', 'test_data.txt' or die 'cat: ' . 'test_data.txt' . ': ' . $! . "\n"; local $/ = undef; my $chunk = <$fh>; close $fh or die 'cat: close failed: ' . $! . "\n"; $chunk; };
my $grep_result_0_1;
my @grep_lines_0_1 = split /\n/msx, $output_0;
my @grep_filtered_0_1 = grep { /Engineer/msx } @grep_lines_0_1;
$grep_result_0_1 = join "\n", @grep_filtered_0_1;
if (!($grep_result_0_1 =~ m{\n\z}msx || $grep_result_0_1 eq q{})) { $grep_result_0_1 .= "\n"; }
$output_0 = $grep_result_0_1;
if ((scalar @grep_filtered_0_1) == 0) { $pipeline_success_0 = 0; }
my @lines_1 = split /\n/msx, $output_0;
my @result_1;
foreach my $line (@lines_1) {
  chomp $line;
  my @fields = split /,/msx, $line;
  my @sel = ();
  if (@fields > 0) { push @sel, $fields[0]; }
  if (@fields > 3) { push @sel, $fields[3]; }
  push @result_1, join(q{,}, @sel);
}
$output_0 = join "\n", @result_1;

my @sort_lines_0_3 = split /\n/msx, $output_0;
sub sort_numeric_0_3 {
    my @a_fields = split /,/msx, $a;
    my @b_fields = split /,/msx, $b;
    my $a_num = 0;
    my $b_num = 0;
    my $a_key = ( scalar @a_fields > 1 ) ? $a_fields[1] : q{}; $a_key =~ s/^\s+|\s+$//g;
    my $b_key = ( scalar @b_fields > 1 ) ? $b_fields[1] : q{}; $b_key =~ s/^\s+|\s+$//g;
    if ( $a_key =~ /^\d+(?:[.]\d+)?$/msx ) { $a_num = $a_key; }
    if ( $b_key =~ /^\d+(?:[.]\d+)?$/msx ) { $b_num = $b_key; }
    return $a_num <=> $b_num || $a cmp $b;
}
my @sort_sorted_0_3 = sort sort_numeric_0_3 @sort_lines_0_3;
@sort_sorted_0_3 = reverse @sort_sorted_0_3;
$output_0 = join "\n", @sort_sorted_0_3;
if ($output_0 ne q{} && !($output_0 =~ m{\n\z}msx)) { $output_0 .= "\n"; }

print $output_0;

#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== Here-string with grep -o ==\n");
my $here_string_content = "some pattern here";
# Redirect HereString not yet implemented
my @here_lines = split(/\n/, $here_string_content);
foreach my $line (@here_lines) {
    if ($line =~ /(pattern)/) {
        print "$1\n";
    }
}
print("== Process substitution with comm ==\n");
my $temp_file_ps_1 = '/tmp/process_sub_0_1.tmp';
open(my $fh, '>', $temp_file_ps_1) or die "Cannot create temp file: $!\n";
close($fh);
my $temp_file_ps_2 = '/tmp/process_sub_1_2.tmp';
open(my $fh, '>', $temp_file_ps_2) or die "Cannot create temp file: $!\n";
close($fh);
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("printf"), args: [Literal("a\\nb\\n")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("printf"), args: [Literal("b\\nc\\n")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
print("== readarray/mapfile ==\n");
my $temp_file_input_1 = '/tmp/process_sub_input_2_1.tmp';
open(my $fh, '>', $temp_file_input_1) or die "Cannot create temp file: $!\n";
print $fh "x
y
";
close($fh);
my @lines = ();
open(my $fh_1, '<', '/tmp/process_sub_input_2_1.tmp') or die "Cannot open file: $!\n";
while (my $line = <$fh_1>) {
    chomp $line;
    push @lines, $line;
}
close($fh_1);
printf("%s ", join(" ", @lines));
print("\n");
print("== More process substitution examples ==\n");
my $pipeline_result_1 = system('diff') == 0 || (print("Files differ\n"));
my $temp_file_ps_1 = '/tmp/process_sub_3_1.tmp';
open(my $fh, '>', $temp_file_ps_1) or die "Cannot create temp file: $!\n";
close($fh);
my $temp_file_ps_2 = '/tmp/process_sub_4_2.tmp';
open(my $fh, '>', $temp_file_ps_2) or die "Cannot create temp file: $!\n";
close($fh);
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("echo"), args: [Literal("-e"), Literal("name1\\nname2")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("echo"), args: [Literal("-e"), Literal("value1\\nvalue2")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
print qx(paste '/tmp/process_sub_3_1.tmp' '/tmp/process_sub_4_2.tmp');
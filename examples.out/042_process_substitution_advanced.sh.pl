#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== More process substitution examples ==\n");
my $pipeline_result_1 = system('diff') == 0 || (print("Files differ\n"));
my $temp_file_ps_1 = '/tmp/process_sub_8_1.tmp';
open(my $fh, '>', $temp_file_ps_1) or die "Cannot create temp file: $!\n";
close($fh);
my $temp_file_ps_2 = '/tmp/process_sub_9_2.tmp';
open(my $fh, '>', $temp_file_ps_2) or die "Cannot create temp file: $!\n";
close($fh);
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("echo"), args: [Literal("-e"), Literal("name1\\nname2")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("echo"), args: [Literal("-e"), Literal("value1\\nvalue2")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
print qx(paste '/tmp/process_sub_8_1.tmp' '/tmp/process_sub_9_2.tmp');
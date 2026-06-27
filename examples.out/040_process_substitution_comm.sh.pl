#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== Process substitution with comm ==\n");
my $temp_file_ps_1 = '/tmp/process_sub_5_1.tmp';
open(my $fh, '>', $temp_file_ps_1) or die "Cannot create temp file: $!\n";
close($fh);
my $temp_file_ps_2 = '/tmp/process_sub_6_2.tmp';
open(my $fh, '>', $temp_file_ps_2) or die "Cannot create temp file: $!\n";
close($fh);
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("printf"), args: [Literal("a\\nb\\n")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
# Redirect ProcessSubstitutionInput(Subshell(Simple(SimpleCommand { name: Literal("printf"), args: [Literal("b\\nc\\n")], redirects: [], env_vars: {}, append_vars: {} }))) not yet implemented
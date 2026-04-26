#!/usr/bin/env perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/000benchmark.pl" }
use strict;
use warnings;
use File::Basename;

my $main_exit_code = 0;

my $i = 0;

{
    my $output_0;
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        my $string = "Line:LINE";
    $output_0 = '';
    for (my $i = 0; $i < 1000; $i++) {
        $output_0 .= "$string\n";
    }

        my $inner_input_0_1 = $output_0;
    my $inner_output_0_1_0;
    my @lines = split(/\n/, $inner_input_0_1);
    my $num_lines = 1000;
    if ($num_lines > scalar(@lines)) {
    $num_lines = scalar(@lines);
    }
    my @result = @lines[0..$num_lines-1];
    $inner_output_0_1_0 = join("\n", @result);
    my $inner_output_0_1_1;
    my @while_lines_0_1_1 = split(/\n/, $inner_output_0_1_0);
    my $result_0_1_1 = '';
    for my $line (@while_lines_0_1_1) {
        chomp $line;
        my $L = $line;
            $i = $i+1;
    {
            my $output_1;
            my $output_printed_1;
            my $pipeline_success_1 = 1;
                    $output_1 .= $L. "\n";
                    my @sed_lines_1 = split(/\n/, $output_1);
            my @sed_result_1;
            foreach my $line (@sed_lines_1) {
            chomp($line);
            $line =~ s/LINE/$i/g;
            push @sed_result_1, $line;
            }
            $output_1 = join("\n", @sed_result_1);
            if ($output_1 ne '' && !defined($output_printed_1)) {
                print $output_1;
                print "\n" unless $output_1 =~ /\n$/;
            }
            $main_exit_code = 1 unless $pipeline_success_1;
            }
    }
    $inner_output_0_1_1 = $result_0_1_1;
    $output_0 = $inner_output_0_1_1;
    if ($output_0 ne '' && !defined($output_printed_0)) {
        print $output_0;
        print "\n" unless $output_0 =~ /\n$/;
    }
    $main_exit_code = 1 unless $pipeline_success_0;
    }

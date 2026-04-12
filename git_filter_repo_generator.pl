#!/usr/bin/env perl

use strict;
use warnings;
use Getopt::Long;
use File::Spec;

# Default values
my $mode = '';
my $help = 0;

# Parse command line options
GetOptions(
    'good' => sub { $mode = 'good' },
    'bad'  => sub { $mode = 'bad' },
    'help' => \$help,
    'h'    => \$help,
) or die "Error in command line arguments\n";

# Show help if requested
if ($help) {
    print <<'EOF';
Usage: $0 [--good|--bad] [--help|-h]

Reads a list of paths from stdin and outputs a git filter-repo command.

Options:
  --good    Treat stdin as a list of directories/files to KEEP (default)
  --bad     Treat stdin as a list of directories/files to REMOVE
  --help    Show this help message
  -h        Show this help message

Examples:
  # Keep only specific directories
  echo -e "src/\ndocs/\ntests/" | $0 --good
  
  # Remove specific directories
  echo -e "temp/\nbuild/\n*.log" | $0 --bad
  
  # Default behavior (same as --good)
  echo -e "important/" | $0

The script outputs a git filter-repo command that can be executed to filter
the repository according to the specified paths.
EOF
    exit 0;
}

# Default to 'good' mode if no mode specified
$mode = 'good' if $mode eq '';

# Read paths from stdin
my @paths = ();
while (<STDIN>) {
    chomp;
    next if /^\s*$/;  # Skip empty lines
    next if /^#/;     # Skip comment lines
    
    # Clean up the path
    s/^\s+|\s+$//g;   # Trim whitespace
    next if $_ eq ''; # Skip empty after trimming
    
    push @paths, $_;
}

if (@paths == 0) {
    die "Error: No paths provided on stdin\n";
}

# Build the git filter-repo command
my $command = "git filter-repo";

if ($mode eq 'good') {
    # Keep only the specified paths
    $command .= " --path-glob";
    foreach my $path (@paths) {
        # Handle different path formats
        if ($path =~ /\/$/) {
            # Directory path - keep everything under it
            $command .= " '$path*'";
        } elsif ($path =~ /\*/) {
            # Already a glob pattern
            $command .= " '$path'";
        } else {
            # File or directory - keep exact match and subdirectory
            $command .= " '$path' '$path/*'";
        }
    }
} else { # $mode eq 'bad'
    # Remove the specified paths
    $command .= " --path-glob";
    foreach my $path (@paths) {
        # Handle different path formats for removal
        if ($path =~ /\/$/) {
            # Directory path - remove everything under it
            $command .= " ':$path*'";
        } elsif ($path =~ /\*/) {
            # Already a glob pattern - prefix with colon for removal
            $command .= " ':$path'";
        } else {
            # File or directory - remove exact match and subdirectory
            $command .= " ':$path' ':$path/*'";
        }
    }
}

# Add common options
$command .= " --force";

# Output the command
print "$command\n";

# Also provide some helpful information
print STDERR "\n# Generated git filter-repo command for ";
print STDERR ($mode eq 'good' ? 'keeping' : 'removing');
print STDERR " the following paths:\n";
foreach my $path (@paths) {
    print STDERR "#   $path\n";
}
print STDERR "#\n";
print STDERR "# To execute this command, run:\n";
print STDERR "#   $command\n";
print STDERR "#\n";
print STDERR "# WARNING: This will rewrite git history. Make sure you have a backup!\n";

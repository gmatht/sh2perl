#!/usr/bin/env python3
"""
Script to find Perl-specific code chunks in the sh2perl codebase.
Searches for code that is specifically related to generating Perl output
rather than other languages that could be generated.
"""

import os
import re
import sys
from pathlib import Path
from typing import List, Dict, Tuple

class PerlCodeFinder:
    def __init__(self, src_dir: str = "src"):
        self.src_dir = Path(src_dir)
        self.perl_patterns = {
            # Function names that explicitly mention Perl
            'perl_functions': [
                r'word_to_perl',
                r'perl_string_literal',
                r'convert.*perl',
                r'generate.*perl',
                r'strip_shell_quotes_and_convert_to_perl',
                r'strip_shell_quotes_for_regex',
                r'convert_string_interpolation_to_perl',
                r'convert_arithmetic_to_perl',
                r'convert_extglob_to_perl_regex'
            ],
            
            # Perl-specific syntax and constructs
            'perl_syntax': [
                r'#!/usr/bin/env perl',
                r'use strict',
                r'use warnings',
                r'use File::',
                r'my \$',
                r'scalar\(',
                r'@ARGV',
                r'\$\{',
                r'opendir\(',
                r'readdir\(',
                r'closedir\(',
                r'stat\(',
                r'printf\(',
                r'join\(',
                r'split\(',
                r'grep\(',
                r'map\(',
                r'sort\(',
                r'keys\(',
                r'values\(',
                r'each\(',
                r'exists\(',
                r'defined\(',
                r'chomp\(',
                r'chop\(',
                r'length\(',
                r'substr\(',
                r'index\(',
                r'rindex\(',
                r'uc\(',
                r'lc\(',
                r'ucfirst\(',
                r'lcfirst\(',
                r'quotemeta\(',
                r'glob\(',
                r'File::Find',
                r'File::Path',
                r'File::Copy',
                r'File::Basename',
                r'File::Glob'
            ],
            
            # Perl-specific variable patterns
            'perl_variables': [
                r'\$main_exit_code',
                r'\$File::Find::name',
                r'\$File::Find::dir',
                r'\$!',
                r'\$\$',
                r'\$0',
                r'\$ARGV\[',
                r'@ARGV',
                r'%ENV',
                r'\$_',
                r'\$\|',
                r'\$\^',
                r'\$\$'
            ],
            
            # Perl-specific control structures
            'perl_control': [
                r'for my \$',
                r'foreach my \$',
                r'while \(my \$',
                r'if \(.*\) \{',
                r'unless \(.*\) \{',
                r'next if',
                r'last if',
                r'redo if',
                r'die "',
                r'warn "',
                r'croak "',
                r'carp "'
            ],
            
            # Perl-specific operators and expressions
            'perl_operators': [
                r'=~ /',
                r'!~ /',
                r'\.=',
                r'\.\.',
                r'x ',
                r'<=>',
                r'cmp',
                r'eq ',
                r'ne ',
                r'lt ',
                r'le ',
                r'gt ',
                r'ge ',
                r'&&',
                r'\|\|',
                r'and ',
                r'or ',
                r'not '
            ],
            
            # Perl-specific string and regex patterns
            'perl_strings': [
                r'q\{',
                r'qq\{',
                r'qw\{',
                r'qr\{',
                r'/\w+/',
                r's/\w+/\w+/',
                r'tr/\w+/\w+/',
                r'y/\w+/\w+/',
                r'\\n',
                r'\\t',
                r'\\r',
                r'\\"',
                r"\\'",
                r'\\\\'
            ]
        }
        
        self.context_lines = 3  # Lines of context to show around matches

    def find_perl_code(self) -> Dict[str, List[Dict]]:
        """Find all Perl-specific code chunks in the source directory."""
        results = {}
        
        if not self.src_dir.exists():
            print(f"Error: Source directory '{self.src_dir}' does not exist")
            return results
            
        for pattern_category, patterns in self.perl_patterns.items():
            results[pattern_category] = []
            
            for pattern in patterns:
                matches = self._search_pattern(pattern, pattern_category)
                results[pattern_category].extend(matches)
        
        return results

    def _search_pattern(self, pattern: str, category: str) -> List[Dict]:
        """Search for a specific pattern in all Rust files."""
        matches = []
        regex = re.compile(pattern, re.IGNORECASE)
        
        for rust_file in self.src_dir.rglob("*.rs"):
            try:
                with open(rust_file, 'r', encoding='utf-8') as f:
                    lines = f.readlines()
                
                for line_num, line in enumerate(lines, 1):
                    if regex.search(line):
                        match_info = {
                            'file': str(rust_file.relative_to(self.src_dir)),
                            'line_number': line_num,
                            'line_content': line.rstrip(),
                            'pattern': pattern,
                            'context': self._get_context(lines, line_num - 1)
                        }
                        matches.append(match_info)
                        
            except Exception as e:
                print(f"Error reading {rust_file}: {e}")
                
        return matches

    def _get_context(self, lines: List[str], match_line: int) -> List[str]:
        """Get context lines around a match."""
        start = max(0, match_line - self.context_lines)
        end = min(len(lines), match_line + self.context_lines + 1)
        
        context = []
        for i in range(start, end):
            prefix = ">>> " if i == match_line else "    "
            context.append(f"{prefix}{i+1:4d}: {lines[i].rstrip()}")
            
        return context

    def print_results(self, results: Dict[str, List[Dict]]):
        """Print the search results in a formatted way."""
        total_matches = sum(len(matches) for matches in results.values())
        
        print(f"Perl-Specific Code Search Results")
        print(f"=================================")
        print(f"Total matches found: {total_matches}")
        print()
        
        for category, matches in results.items():
            if not matches:
                continue
                
            print(f"{category.replace('_', ' ').title()} ({len(matches)} matches)")
            print("-" * 50)
            
            for match in matches:
                print(f"File: {match['file']}:{match['line_number']}")
                print(f"Pattern: {match['pattern']}")
                print("Context:")
                for context_line in match['context']:
                    try:
                        print(context_line)
                    except UnicodeEncodeError:
                        # Handle encoding issues by replacing problematic characters
                        safe_line = context_line.encode('ascii', 'replace').decode('ascii')
                        print(safe_line)
                print()
            
            print()

    def save_results(self, results: Dict[str, List[Dict]], output_file: str = "perl_specific_code.txt"):
        """Save results to a text file."""
        with open(output_file, 'w', encoding='utf-8') as f:
            f.write("Perl-Specific Code Search Results\n")
            f.write("=================================\n\n")
            
            total_matches = sum(len(matches) for matches in results.values())
            f.write(f"Total matches found: {total_matches}\n\n")
            
            for category, matches in results.items():
                if not matches:
                    continue
                    
                f.write(f"{category.replace('_', ' ').title()} ({len(matches)} matches)\n")
                f.write("-" * 50 + "\n")
                
                for match in matches:
                    f.write(f"File: {match['file']}:{match['line_number']}\n")
                    f.write(f"Pattern: {match['pattern']}\n")
                    f.write("Context:\n")
                    for context_line in match['context']:
                        f.write(f"{context_line}\n")
                    f.write("\n")
                
                f.write("\n")

def main():
    """Main function to run the Perl code finder."""
    import argparse
    
    parser = argparse.ArgumentParser(description="Find Perl-specific code in sh2perl codebase")
    parser.add_argument("--src-dir", default="src", help="Source directory to search (default: src)")
    parser.add_argument("--output", "-o", help="Output file to save results")
    parser.add_argument("--category", help="Specific category to search (perl_functions, perl_syntax, etc.)")
    
    args = parser.parse_args()
    
    finder = PerlCodeFinder(args.src_dir)
    results = finder.find_perl_code()
    
    # Filter by category if specified
    if args.category and args.category in results:
        results = {args.category: results[args.category]}
    
    # Print results
    finder.print_results(results)
    
    # Save to file if requested
    if args.output:
        finder.save_results(results, args.output)
        print(f"Results saved to {args.output}")

if __name__ == "__main__":
    main()
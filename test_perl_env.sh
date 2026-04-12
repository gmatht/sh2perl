#!/bin/bash

export LOCALE=C
export LC_COLLATE=C
export PATH="/c/Strawberry/perl/bin:/c/Program Files/Git/usr/bin:$PATH"

echo "PATH: $PATH"
echo "Perl location:"
which perl || echo "perl not found in PATH"
echo "Direct perl check:"
/c/Strawberry/perl/bin/perl.exe -e "print 'Perl is working\n'"
echo "PPI check:"
/c/Strawberry/perl/bin/perl.exe -e "use PPI; print 'PPI is available\n'"


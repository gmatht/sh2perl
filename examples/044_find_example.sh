#!/bin/bash

# Find all .txt files in current directory and subdirectories
find . -name "*.txt" -type f | sort

# Find files modified in the last 7 days
find . -mtime -7 -type f  | sort

# Find files modified in the last 1 day
find . -mtime -1 -type f  | sort

# Find files modified in the last 1 hour
find . -mmin -60 -type f  | sort

# Find files larger than 1MB
find . -size +1M -type f  | sort

# Find empty files and directories
find . -empty  | sort

# Don't use  yet, they are not portable
# Find files with specific permissions (executable)
# find . -perm -u+x -type f

# Find files by owner
#find . -user $USER -type f

# Find files by group
#find . -group $(id -gn) -type f

# Find files and execute command on them
touch a.logtmp a.logtmp.sav
find . -name "*.logtmp" -exec rm {} \;

ls *.logtmp*

rm a.logtmp.sav

# Find files and show detailed information
find . -type f -ls  | sort

# Find files excluding certain directories
find .. -type f -not -path "./.git/*" -not -path "./node_modules/*"  | sort

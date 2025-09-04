#!/bin/bash

# Find all .txt files in current directory and subdirectories
echo '#find . -name "*.txt" -type f | sort'
find . -name "*.txt" -type f | sort

# Find files modified in the last 7 days
echo '
find . -mtime -7 -type f  | sort'
find . -mtime -7 -type f  | sort

# Find files modified in the last 1 day
echo '
find . -mtime -1 -type f  | sort'
find . -mtime -1 -type f  | sort

# Find files modified in the last 1 hour
# dd
echo '
find . -mmin -60 -type f  | sort'
find . -mmin -60 -type f  | sort

# Find files larger than 1MB
echo '
find . -size +1M -type f  | sort'
find . -size +1M -type f  | sort

# Find empty files and directories
echo '
find . -empty  | sort'
find . -empty  | sort

# Don't use  yet, they are not portable
# Find files with specific permissions (executable)
# find . -perm -u+x -type f

# Find files by owner
#find . -user $USER -type f

# Find files by group
#find . -group $(id -gn) -type f

# Find files and execute command on them
echo 'touch/ls/rm'
touch a.logtmp a.logtmp.sav
find . -name "*.logtmp" -exec rm {} \;

ls *.logtmp*

rm a.logtmp.sav

# Find files and show detailed information
#echo 'find . -type f -ls  | sort'
#find . -type f -ls  | sort

# Find files excluding certain directories
echo 'find .. -type f -not -path "./.git/*" -not -path "./node_modules/*"  | sort'
find .. -type f -not -path "./.git/*" -not -path "./node_modules/*"  | sort

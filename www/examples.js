// Shell script examples for the Debashc compiler
// This file contains all examples that were previously embedded in WASM
// Generated automatically from examples/ directory
// Categorized based on test results from results/* files

export const examples = {
  '000__01_file_directory_operations.sh': `\#!/bin/bash

\# File and directory operations with backticks
\# This file demonstrates using backticks with file system commands

echo "=== File and Directory Operations ==="

\# ls command with backticks
\#PERL_MUST_NOT_CONTAIN \`ls
file_list=\`ls -a\`
echo "File listing:"
echo "\$file_list"

\# find command with backticks
\#PERL_MUST_NOT_CONTAIN \`find
found_files=\`find . -maxdepth 1 -name "*.sh" -type f\`
echo "Found shell scripts:"
echo "\$found_files"

\# basename and dirname with backticks
\#PERL_MUST_NOT_CONTAIN \`basename
\#PERL_MUST_NOT_CONTAIN \`dirname
\#script_name=\`basename \$0\`
\#script_dir=\`dirname \$0\`
\#echo "Script name: \$script_name"
\#echo "Script directory: \$script_dir"
`,
  '000__02_output_formatting_commands.sh': `\#!/bin/bash

\# Output and formatting commands with backticks
\# This file demonstrates using backticks with output and formatting commands

echo "=== Output and Formatting Commands ==="

\# echo command with backticks
\#PERL_MUST_NOT_CONTAIN \`echo
echo_result=\`echo "Hello from backticks"\`
echo "Echo result: \$echo_result"

\# printf command with backticks
\#PERL_MUST_NOT_CONTAIN \`printf
printf_result=\`printf "Number: %d, String: %s\\n" 42 "test"\`
echo "Printf result: \$printf_result"

echo "=== Compression Commands ==="

\# gzip command with backticks
\#PERL_MUST_NOT_CONTAIN \`gzip
\#echo "test content for compression" > test_compress.txt
\#gzip_result=\`gzip test_compress.txt && echo "Compression successful"\`
\#echo "Gzip result: \$gzip_result"

\# zcat command with backticks
\#PERL_MUST_NOT_CONTAIN \`zcat
\#zcat_result=\`zcat test_compress.txt.gz\`
\#echo "Zcat result: \$zcat_result"

echo "=== Network Commands ==="

\# wget command with backticks
\#PERL_MUST_NOT_CONTAIN \`wget
\# wget_result=\`wget -qO- http://httpbin.org/get | head -1\`
\# echo "Wget result: \$wget_result"

\# curl command with backticks
\#PERL_MUST_NOT_CONTAIN \`curl
\# curl_result=\`curl -s http://httpbin.org/get | head -1\`
\# echo "Curl result: \$curl_result"

echo "=== Process Management Commands ==="

\# kill command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`kill
\# kill_result=\`kill -0 \$\$ && echo "Process exists"\`
\# echo "Kill result: \$kill_result"

\# nohup command with backticks
\#PERL_MUST_NOT_CONTAIN \`nohup
\# nohup_result=\`nohup echo "background process" 2>&1\`
\# echo "Nohup result: \$nohup_result"

\# nice command with backticks
\#PERL_MUST_NOT_CONTAIN \`nice
\#nice_result=\`nice echo "low priority process"\`
\#echo "Nice result: \$nice_result"

echo "=== Checksum Commands ==="

\# sha256sum command with backticks
\#PERL_MUST_NOT_CONTAIN \`sha256sum
echo "test content" > test_checksum.txt
sha256_result=\`sha256sum test_checksum.txt\`
echo "SHA256 result: \$sha256_result"

\# sha512sum command with backticks
\#PERL_MUST_NOT_CONTAIN \`sha512sum
sha512_result=\`sha512sum test_checksum.txt\`
echo "SHA512 result: \$sha512_result"

\# strings command with backticks
\#PERL_MUST_NOT_CONTAIN \`strings
strings_result=\`strings test_binary.txt | head -3\`
echo "Strings result:"
echo "\$strings_result"

echo "=== I/O Redirection Commands ==="

\# tee command with backticks
\#PERL_MUST_NOT_CONTAIN \`tee
tee_result=\`echo "test output" | tee test_tee.txt\`
echo "Tee result: \$tee_result"

echo "=== Perl Command ==="

\# perl command with backticks
\#PERL_MUST_NOT_CONTAIN \`perl
perl_result=\`perl -e 'print "Hello from Perl\\n"'\`
echo "Perl result: \$perl_result"

\# Cleanup
rm -f test_checksum.txt test_tee.txt

`,
  '000__03_file_manipulation_commands.sh': `\#!/bin/bash

\# File manipulation commands with backticks
\# This file demonstrates using backticks with file manipulation commands

echo "=== File Manipulation Commands ==="

echo "=== cp command ==="
\# cp command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`cp
echo
echo "test content" > test_file.txt
cp_result=\`cp test_file.txt test_file_copy.txt && echo "Copy successful"\`
echo "Copy result: \$cp_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

echo
echo "=== mv command ==="
\# mv command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`mv
mv_result=\`mv test_file_copy.txt test_file_moved.txt && echo "Move successful"\`
echo "Move result: \$mv_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

echo
echo "=== rm command ==="
\# rm command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`rm
rm_result=\`rm test_file.txt test_file_moved.txt && echo "Remove successful"\`
echo "Remove result: \$rm_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

echo
echo "=== mkdir command ==="
\# mkdir command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`mkdir
mkdir_result=\`mkdir test_dir && echo "Directory created"\`
echo "Mkdir result: \$mkdir_result"
touch test_dir/file
ls test_dir 2>/dev/null || echo "Directory not found"
rm test_dir/file
rmdir test_dir

echo
echo "=== touch command ==="
\# touch command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`touch
touch_result=\`touch test_file.txt && echo "File touched"\`
echo "Touch result: \$touch_result"

echo
\# Cleanup
rm -f test_file.txt test_file_copy.txt test_file_moved.txt
rm -rf test_dir 2>/dev/null || true

`,
  '000__04a_basic_command_substitution.sh': `\#!/bin/bash

\# Basic command substitution examples using backticks
\# This file demonstrates simple command substitution using backticks (\`)

echo "=== Basic Command Substitution ==="

\# Simple command substitution
echo "Current date: \`date +%Y\`"
\#echo "Current user: \`whoami\`"
echo "Current directory: \`basename \$(pwd)\`"

\# Assigning backtick results to variables
current_date=\`date +%Y%m\`
\#current_user=\`whoami\`
current_dir=\`basename \$(pwd)\`

echo "Stored date: \$current_date"
\#echo "Stored user: \$current_user"
echo "Stored directory: \$current_dir"

echo "=== Basic Command Substitution Complete ==="

`,
  '000__04b_file_directory_operations.sh': `\#!/bin/bash

\# File and directory operations using backticks
\# This file demonstrates file and directory commands with backticks

echo "=== File and Directory Operations ==="

\# ls command with backticks
\#PERL_MUST_NOT_CONTAIN \`ls
file_list=\`ls -a\`
echo "File listing:"
echo "\$file_list"

\# find command with backticks
\#PERL_MUST_NOT_CONTAIN \`find
found_files=\`find . -name "*.sh" -type f\`
echo "Found shell scripts:"
echo "\$found_files"

\# basename and dirname with backticks
\#PERL_MUST_NOT_CONTAIN \`basename
\#PERL_MUST_NOT_CONTAIN \`dirname
\#script_name=\`basename \$0\`
\#script_dir=\`dirname \$0\`
\#echo "Script name: \$script_name"
\#echo "Script directory: \$script_dir"

echo "=== File and Directory Operations Complete ==="

`,
  '000__04c_text_processing_commands.sh': `\#!/bin/bash

\# Text processing commands using backticks
\# This file demonstrates various text processing commands with backticks

echo "=== Text Processing Commands ==="

\# cat command with backticks
\#PERL_MUST_NOT_CONTAIN \`cat
file_content=\`cat 000__04c_text_processing_commands.sh | head -5\`
echo "First 5 lines of this file:"
echo "\$file_content"

\# grep command with backticks
\#PERL_MUST_NOT_CONTAIN \`grep
grep_result=\`grep -n "echo" 000__04c_text_processing_commands.sh\`
echo "Lines containing 'echo':"
echo "\$grep_result"

\# sed command with backticks
\#PERL_MUST_NOT_CONTAIN \`sed
sed_result=\`echo "Hello World" | sed 's/World/Universe/'\`
echo "Sed result: \$sed_result"

\# awk command with backticks
\#PERL_MUST_NOT_CONTAIN \`awk
awk_result=\`echo "1 2 3 4 5" | awk '{print \$1 + \$2}'\`
echo "Awk sum result: \$awk_result"

\# sort command with backticks
\#PERL_MUST_NOT_CONTAIN \`sort
sort_result=\`echo -e "zebra\\napple\\nbanana" | sort\`
echo "Sorted words:"
echo "\$sort_result"

\# uniq command with backticks
\#PERL_MUST_NOT_CONTAIN \`uniq
uniq_result=\`echo -e "apple\\napple\\nbanana\\nbanana\\ncherry" | uniq\`
echo "Unique words:"
echo "\$uniq_result"

\# wc command with backticks
\#PERL_MUST_NOT_CONTAIN \`wc
word_count=\`echo "Hello World" | wc -w\`
line_count=\`echo -e "line1\\nline2\\nline3" | wc -l\`
echo "Word count: \$word_count"
echo "Line count: \$line_count"

\# head command with backticks
\#PERL_MUST_NOT_CONTAIN \`head
head_result=\`seq 1 10 | head -3\`
echo "First 3 numbers: \$head_result"

\# tail command with backticks
\#PERL_MUST_NOT_CONTAIN \`tail
tail_result=\`seq 1 10 | tail -3\`
echo "Last 3 numbers: \$tail_result"

\# cut command with backticks
\#PERL_MUST_NOT_CONTAIN \`cut
cut_result=\`echo "apple:banana:cherry" | cut -d: -f2\`
echo "Second field: \$cut_result"

\# paste command with backticks
\#PERL_MUST_NOT_CONTAIN \`paste
echo -e "1\\n2\\n3" > temp1.txt
echo -e "a\\nb\\nc" > temp2.txt
paste_result=\`paste temp1.txt temp2.txt\`
echo "Pasted columns:"
echo "\$paste_result"
rm -f temp1.txt temp2.txt

\# comm command with backticks
\#PERL_MUST_NOT_CONTAIN \`comm
echo -e "apple\\nbanana\\ncherry" > file1.txt
echo -e "banana\\ncherry\\ndate" > file2.txt
comm_result=\`comm -12 file1.txt file2.txt\`
echo "Common lines:"
echo "\$comm_result"

\# diff command with backticks
\#PERL_MUST_NOT_CONTAIN \`diff
diff_result=\`diff file1.txt file2.txt\`
echo "File differences:"
echo "\$diff_result"

\# tr command with backticks
\#PERL_MUST_NOT_CONTAIN \`tr
tr_result=\`echo "HELLO WORLD" | tr 'A-Z' 'a-z'\`
echo "Lowercase: \$tr_result"

\# xargs command with backticks
\#PERL_MUST_NOT_CONTAIN \`xargs
xargs_result=\`echo "1 2 3" | xargs -n1 echo "Number:"\`
echo "Xargs result:"
echo "\$xargs_result"

\# Cleanup
rm -f file1.txt file2.txt

echo "=== Text Processing Commands Complete ==="
`,
  '000__04d_system_utilities.sh': `\#!/bin/bash

\# System utilities using backticks
\# This file demonstrates system utility commands with backticks

echo "=== System Utilities ==="

\# date command with backticks - use fixed format to avoid timing issues
\#PERL_MUST_NOT_CONTAIN \`date
\#timestamp=\`date +%H:%M:%S\`
formatted_date=\`date '+%Y-%m-%d'\`
\#echo "Timestamp: \$timestamp"
echo "Formatted date: \$formatted_date"

\# time command with backticks - use a simple test that doesn't vary much
\#PERL_MUST_NOT_CONTAIN \`time
\#time_result=\`time echo "test" 2>&1 | sed 's/...\$//'\`
\#echo "Time result: \$time_result"

\# sleep command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`sleep
sleep_duration=\`echo "1"\`
echo "Sleeping for \$sleep_duration seconds..."
sleep \$sleep_duration

\# which command with backticks
\#PERL_MUST_NOT_CONTAIN \`which
\#bash_path=\`which bash\`
\#echo "Bash path: \$bash_path"

\# yes command with backticks
\#PERL_MUST_NOT_CONTAIN \`yes
yes_result=\`yes "Hello" | head -3\`
echo "Yes command result:"
echo "\$yes_result"

echo "=== System Utilities Complete ==="

`,
  '000__04e_file_manipulation.sh': `\#!/bin/bash

\# File manipulation commands using backticks
\# This file demonstrates file manipulation commands with backticks

echo "=== File Manipulation Commands ==="

\# cp command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`cp
echo "test content" > test_file.txt
cp_result=\`cp test_file.txt test_file_copy.txt && echo "Copy successful"\`
echo "Copy result: \$cp_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

\# mv command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`mv
mv_result=\`mv test_file_copy.txt test_file_moved.txt && echo "Move successful"\`
echo "Move result: \$mv_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

\# rm command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`rm
rm_result=\`rm test_file.txt test_file_moved.txt && echo "Remove successful"\`
echo "Remove result: \$rm_result"
ls test_file.txt test_file_copy.txt test_file_moved.txt 2>/dev/null || echo "No test files found"

\# mkdir command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`mkdir
mkdir_result=\`mkdir test_dir && echo "Directory created"\`
echo "Mkdir result: \$mkdir_result"
touch test_dir/file
ls test_dir 2>/dev/null || echo "Directory not found"

\# touch command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`touch
touch_result=\`touch test_file.txt && echo "File touched"\`
echo "Touch result: \$touch_result"

\# Cleanup
rm -f test_file.txt test_file_copy.txt test_file_moved.txt
rm -rf test_dir 2>/dev/null || true

echo "=== File Manipulation Commands Complete ==="

`,
  '000__04f_output_formatting.sh': `\#!/bin/bash

\# Output and formatting commands using backticks
\# This file demonstrates output and formatting commands with backticks

echo "=== Output and Formatting Commands ==="

\# echo command with backticks
\#PERL_MUST_NOT_CONTAIN \`echo
echo_result=\`echo "Hello from backticks"\`
echo "Echo result: \$echo_result"

\# printf command with backticks
\#PERL_MUST_NOT_CONTAIN \`printf
printf_result=\`printf "Number: %d, String: %s\\n" 42 "test"\`
echo "Printf result: \$printf_result"

\# tee command with backticks
\#PERL_MUST_NOT_CONTAIN \`tee
tee_result=\`echo "test output" | tee test_tee.txt\`
echo "Tee result: \$tee_result"

\# Cleanup
rm -f test_tee.txt

echo "=== Output and Formatting Commands Complete ==="

`,
  '000__04g_checksum_commands.sh': `\#!/bin/bash

\# Checksum commands using backticks
\# This file demonstrates checksum and related commands with backticks

echo "=== Checksum Commands ==="

\# sha256sum command with backticks
\#PERL_MUST_NOT_CONTAIN \`sha256sum
echo "test content" > test_checksum.txt
sha256_result=\`sha256sum test_checksum.txt\`
echo "SHA256 result: \$sha256_result"

\# sha512sum command with backticks
\#PERL_MUST_NOT_CONTAIN \`sha512sum
sha512_result=\`sha512sum test_checksum.txt\`
echo "SHA512 result: \$sha512_result"

\# strings command with backticks
\#PERL_MUST_NOT_CONTAIN \`strings
strings_result=\`strings target/debug/debashc.exe | head -3\`
echo "Strings result:"
echo "\$strings_result"

\# Cleanup
rm -f test_checksum.txt

echo "=== Checksum Commands Complete ==="

`,
  '000__04h_complex_examples.sh': `\#!/bin/bash

\# Complex backtick examples
\# This file demonstrates complex usage patterns with backticks

echo "=== Complex Backtick Examples ==="

\# Nested backticks
nested_result=\`echo "Three wells: \\\`yes well | head -3\\\`"\`
echo "Nested backticks: \$nested_result"

\# Backticks in arithmetic
count=\`ls -1 | wc -l\`
echo "File count: \$count"

\# Backticks in conditional
current_user=\`echo root\`
if [ "\$current_user" = "root" ]; then
    echo "Running as root"
else
    echo "Not running as root"
fi

\# Backticks in case statement
system_name='Darwin'
case \$system_name in
    Linux)
        echo "Running on Linux"
        ;;
    Darwin)
        echo "Running on macOS"
        ;;
    *)
        echo "Running on other system"
        ;;
esac

\# Backticks in function
get_file_size() {
    local file=\$1
    local size=\`wc -c < "\$file"\`
    echo "File \$file has \$size bytes"
}

get_file_size 000__01_file_directory_operations.sh

\# Backticks in array
files=(\`ls -1 *.sh examples/*.sh 2>/dev/null\`)
echo "Shell scripts found: \${\#files[@]}"
for file in "\${files[@]}"; do
    echo "  - \$file"
done

\# Backticks with process substitution
echo -e "apple\\nbanana\\ncherry" > file1.txt
echo -e "banana\\ncherry\\ndate" > file2.txt
process_result=\`comm -23 <(sort file1.txt) <(sort file2.txt)\`
echo "Process substitution result:"
echo "\$process_result"

\# Backticks with here strings
here_string_result=\`tr 'a-z' 'A-Z' <<< "hello world"\`
echo "Here string result: \$here_string_result"

\# perl command with backticks
\#PERL_MUST_NOT_CONTAIN \`perl
perl_result=\`perl -e 'print "Hello from Perl\\n"'\`
echo "Perl result: \$perl_result"

\# Cleanup
rm -f file1.txt file2.txt

echo "=== Complex Backtick Examples Complete ==="

`,
  '000__05_system_utilities.sh': `\#!/bin/bash

\# System utilities with backticks
\# This file demonstrates using backticks with system utility commands

echo "=== System Utilities ==="

\# date command with backticks
\#PERL_MUST_NOT_CONTAIN \`date
\#timestamp=\`date +%r\`
formatted_date=\`date '+%Y-%m-%d'\`
\#echo "Timestamp: \$timestamp"
echo "Formatted date: \$formatted_date"

\# time command with backticks
\#PERL_MUST_NOT_CONTAIN \`time
\#time_result=\`(time sleep 1) 2>&1 | sed s/...\$//\`
\#echo "Time result: \$time_result"

\# sleep command with backticks (though it doesn't produce output)
\#PERL_MUST_NOT_CONTAIN \`sleep
\#sleep_duration=\`echo "2"\`
\#echo "Sleeping for \$sleep_duration seconds..."
\#sleep \$sleep_duration

\# which command with backticks
\#PERL_MUST_NOT_CONTAIN \`which
\#bash_path=\`which bash\`
\#echo "Bash path: \$bash_path"

\# yes command with backticks
\#PERL_MUST_NOT_CONTAIN \`yes
yes_result=\`yes "Hello" | head -3\`
echo "Yes command result:"
echo "\$yes_result"

`,
  '000__06_text_processing_commands.sh': `\#!/bin/bash

\# Text processing commands with backticks
\# This file demonstrates using backticks with text manipulation commands

echo "=== Text Processing Commands ==="

\# cat command with backticks
\#PERL_MUST_NOT_CONTAIN \`cat
file_content=\`cat src/main.rs | head -5\`
echo "First 5 lines of main.rs:"
echo "\$file_content"

\# grep command with backticks
\#PERL_MUST_NOT_CONTAIN \`grep
grep_result=\`grep -n "fn" src/main.rs\`
echo "Lines containing 'fn':"
echo "\$grep_result"

\# sed command with backticks
\#PERL_MUST_NOT_CONTAIN \`sed
sed_result=\`echo "Hello World" | sed 's/World/Universe/'\`
echo "Sed result: \$sed_result"

\# awk command with backticks
\#PERL_MUST_NOT_CONTAIN \`awk
awk_result=\`echo "1 2 3 4 5" | awk '{print \$1 + \$2}'\`
echo "Awk sum result: \$awk_result"

\# sort command with backticks
\#PERL_MUST_NOT_CONTAIN \`sort
sort_result=\`echo -e "zebra\\napple\\nbanana" | sort\`
echo "Sorted words:"
echo "\$sort_result"

\# uniq command with backticks
\#PERL_MUST_NOT_CONTAIN \`uniq
uniq_result=\`echo -e "apple\\napple\\nbanana\\nbanana\\ncherry" | uniq\`
echo "Unique words:"
echo "\$uniq_result"

\# wc command with backticks
\#PERL_MUST_NOT_CONTAIN \`wc
word_count=\`echo "Hello World" | wc -w\`
line_count=\`echo -e "line1\\nline2\\nline3" | wc -l\`
echo "Word count: \$word_count"
echo "Line count: \$line_count"

\# head command with backticks
\#PERL_MUST_NOT_CONTAIN \`head
head_result=\`seq 1 10 | head -3\`
echo "First 3 numbers: \$head_result"

\# tail command with backticks
\#PERL_MUST_NOT_CONTAIN \`tail
tail_result=\`seq 1 10 | tail -3\`
echo "Last 3 numbers: \$tail_result"

\# cut command with backticks
\#PERL_MUST_NOT_CONTAIN \`cut
cut_result=\`echo "apple:banana:cherry" | cut -d: -f2\`
echo "Second field: \$cut_result"

\# paste command with backticks
\#PERL_MUST_NOT_CONTAIN \`paste
echo -e "1\\n2\\n3" > temp1.txt
echo -e "a\\nb\\nc" > temp2.txt
paste_result=\`paste temp1.txt temp2.txt | sed 's/\\t/ /g'\`
echo "Pasted columns:"
echo "\$paste_result"
rm -f temp1.txt temp2.txt

\# comm command with backticks
\#PERL_MUST_NOT_CONTAIN \`comm
echo -e "apple\\nbanana\\ncherry" > file1.txt
echo -e "banana\\ncherry\\ndate" > file2.txt
comm_result=\`comm -12 file1.txt file2.txt\`
echo "Common lines:"
echo "\$comm_result"

\# diff command with backticks
\#PERL_MUST_NOT_CONTAIN \`diff
diff_result=\`diff file1.txt file2.txt\`
echo "File differences:"
echo "\$diff_result"

\# tr command with backticks
\#PERL_MUST_NOT_CONTAIN \`tr
tr_result=\`echo "HELLO WORLD" | tr 'A-Z' 'a-z'\`
echo "Lowercase: \$tr_result"

\# xargs command with backticks
\#PERL_MUST_NOT_CONTAIN \`xargs
xargs_result=\`echo "1 2 3" | xargs -n1 echo "Number:"\`
echo "Xargs result:"
echo "\$xargs_result"

\# Cleanupcd
rm -f file1.txt file2.txt

`,
  '000__07_find_path_commands.sh': `\#!/bin/bash

\# find command with backticks
\#PERL_MUST_NOT_CONTAIN \`find
found_files=\`find . -name "*.sh" -type f\`
echo "Found shell scripts:"
echo "\$found_files"

\# basename and dirname with backticks
\#PERL_MUST_NOT_CONTAIN \`basename
\#PERL_MUST_NOT_CONTAIN \`dirname
\#script_name=\`basename \$0\`
\#script_dir=\`dirname \$0\`
\#echo "Script name: \$script_name"
\#echo "Script directory: \$script_dir"
`,
  '001_simple.sh': `\#!/bin/bash

\# This script demonstrates basic shell functionality
echo "Hello, World!"

\# Valid if statement
if [ -f "test.txt" ]; then
    echo "File exists"
fi

\# Valid for loop
for i in {1..5}; do
    echo \$i
done 

\#Bash leaves \$i as 5 after the loop. But it is messy to add this if i will not be used later.
\#PERL_MUST_NOT_CONTAIN: \$i = 5;

\#Only use basename if actually needed.
\#PERL_MUST_NOT_CONTAIN: basename

\# "Hello, World!\\n" is simpler
\#PERL_MUST_NOT_CONTAIN: "Hello, World!", "\\n"
\#PERL_MUST_CONTAIN: "Hello, World!\\n"

`,
  '002_control_flow.sh': `\#!/bin/bash

\# Control flow examples
if [ -f "file.txt" ]; then
    echo "File exists"
else
    echo "File does not exist"
fi

for i in {1..5}; do
    echo "Number: \$i"
done

while [ \$i -lt 10 ]; do
    echo "Counter: \$i"
    i=\$((i + 1))
done

function greet() {
    echo "Hello, \$1!"
}

greet "World" `,
  '003_pipeline.sh': `\#!/bin/bash

\# Pipeline examples
ls | grep "\\.txt\$" | wc -l
echo
cat file.txt | sort | uniq -c | sort -nr
echo
find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\\\\\/"
echo
\# This pipeline will use line-by-line processing:
cat file.txt | tr 'a' 'b' | grep 'hello'
echo
\# This pipeline will fall back to buffered processing:
cat file.txt | sort | grep 'hello'`,
  '004_test_quoted.sh': `echo "Hello, World!"
echo 'Single quoted'
echo "String with \\"escaped\\" quotes"
echo "String with 'single' quotes"
`,
  '005_args.sh': `\#!/usr/bin/env bash

\# Demonstrates reading command-line arguments
\# This example is intentionally simple so it parses cleanly

echo "== Argument count =="
echo "\$\#"

echo "== Arguments =="
for a in "\$@"; do
  echo "Arg: \$a"
done



`,
  '006_misc.sh': `\#!/usr/bin/env bash

echo "== Subshell =="
( echo inside-subshell )

echo "== Simple pipeline =="
echo "alpha beta" | grep beta


`,
  '007_cat_EOF.sh': `cat <<EOF
alpha
beta
gamma ...
EOF

cat <<FISH
oyster
snapper
salmon
FISH

echo "Fin. That is all folks."
`,
  '008_simple_backup.sh': `\#!/bin/bash

\# Simple shell script example
echo "Hello, World!"
\#TODO: Support multi-column output
ls -1 | grep -v __tmp_test_output.pl
\#This should be a single token, not two.
\#AST_MUST_CONTAIN: [Literal("-1")]
echo \`ls | grep -v __tmp_test_output.pl\`
\#Lets not consider ls -la at the moment as permissions are OS dependent
\#ls -la
\#grep "pattern" file.txt `,
  '009_arrays.sh': `\#!/usr/bin/env bash

\# Array examples - indexed and associative arrays
\# Demonstrates basic array operations in Bash

set -euo pipefail

echo "== Indexed arrays =="
arr=(one two three)
echo "\${arr[1]}"        \# two
echo "\${\#arr[@]}"       \# 3
for x in "\${arr[@]}"; do printf "%s " "\$x"; done; echo

echo "== Associative arrays =="
declare -A map
map[foo]=bar
map[answer]=42
map[two]="1 + 1"
echo "\${map[foo]}"      \# bar
echo "\${map[answer]}"   \# 42

\# Show all keys and values
for k in "\${!map[@]}"; do echo "\$k => \${map[\$k]}"; done | sort \#Do not care about the order of the elements?
`,
  '010_pattern_matching.sh': `\#!/usr/bin/env bash

\# Pattern matching and regex examples
\# Demonstrates [[ ]] test operator with patterns and regex

set -euo pipefail

echo "== [[ pattern and regex ]]"
s="file.txt"
[[ \$s == *.txt ]] && echo pattern-match
[[ \$s =~ ^file\\.[a-z]+\$ ]] && echo regex-match

echo "== extglob =="
shopt -s extglob
f1="file.js"; f2="thing.min.js"
[[ \$f1 == !(*.min).js ]] && echo f1-ok
[[ \$f2 == !(*.min).js ]] || echo f2-filtered

echo "== nocasematch =="
shopt -s nocasematch
word="Foo"; [[ \$word == foo ]] && echo ci-match
`,
  '011_brace_expansion.sh': `\#!/usr/bin/env bash

\# Brace expansion examples
\# Demonstrates various brace expansion patterns in Bash

set -euo pipefail

echo "== Basic brace expansion =="
echo {1..5}
echo {a..c}
echo {00..04..2}

echo "== Advanced brace expansion =="
echo {a,b,c}{1,2,3}
echo {1..10..2}
echo {a..z..3}

echo "== Practical examples =="
\# Create numbered files
touch file_{001..005}.txt
ls file_*.txt
rm file_*.txt
`,
  '012_process_substitution.sh': `\#!/usr/bin/env bash

\# Process substitution and here-strings
\# Demonstrates advanced input/output redirection in Bash

set -euo pipefail

echo "== Here-string with grep -o =="
grep -o pattern <<< "some pattern here"

echo "== Process substitution with comm =="
comm -12 <(printf 'a\\nb\\n') <(printf 'b\\nc\\n')

echo "== readarray/mapfile =="
mapfile -t lines < <(printf 'x\\ny\\n')
printf '%s ' "\${lines[@]}"; echo

echo "== More process substitution examples =="
\# Compare sorted outputs
diff <(echo -e "a\\nc\\nb" | sort) <(echo -e "a\\nb\\nd" | sort) || echo "Files differ"

\# Use paste with process substitution
paste <(echo -e "name1\\nname2") <(echo -e "value1\\nvalue2")
`,
  '013_parameter_expansion.sh': `\#!/usr/bin/env bash

\# Parameter expansion examples
\# Demonstrates advanced parameter manipulation in Bash

set -euo pipefail

echo "== Case modification in parameter expansion =="
name="world"
echo "\${name^^}"        \# WORLD
echo "\${name,,}"        \# world
echo "\${name^}"         \# World

echo "== Advanced parameter expansion =="
path="/tmp/file.txt"
echo "\${path\#\#*/}"       \# file.txt
echo "\${path%/*}"        \# /tmp
s2="abba"; echo "\${s2//b/X}"  \# aXXa

echo "== More parameter expansion =="
var="hello world"
echo "\${var\#hello}"      \#  world
echo "\${var%world}"      \# hello 
echo "\${var//o/0}"       \# hell0 w0rld

echo "== Default values =="
unset maybe
echo "\${maybe:-default}"  \# default
echo "\${maybe:=default}"  \# default (and sets maybe)
echo "\${maybe:?error}"    \# error if unset
`,
  '014_ansi_quoting.sh': `\#!/usr/bin/env bash

\# ANSI-C quoting and special character examples
\# Demonstrates escape sequences and special character handling

set -euo pipefail

echo "== ANSI-C quoting =="
echo \$'line1\\nline2\\tTabbed'

echo "== Escape sequences =="
echo \$'bell\\a'
echo \$'backspace\\b'
echo \$'formfeed\\f'
echo \$'newline\\n'
echo \$'carriage\\rreturn'
echo \$'tab\\tseparated'
echo \$'vertical\\vtab'

echo "== Unicode and hex =="
echo \$'\\u0048\\u0065\\u006c\\u006c\\u006f'  \# Hello
echo \$'\\x48\\x65\\x6c\\x6c\\x6f'            \# Hello

echo "== Practical examples =="
\# Create a formatted table
printf \$'%-10s %-10s %s\\n' "Name" "Age" "City"
printf \$'%-10s %-10s %s\\n' "John" "25" "NYC"
printf \$'%-10s %-10s %s\\n' "Jane" "30" "LA"
`,
  '015_grep_advanced.sh': `\#!/bin/bash

\# Advanced grep features and options
\# Demonstrates specialized grep capabilities

\# Limit number of matches per file
echo -e "match1\\nmatch2\\nmatch3\\nmatch4" | grep -m 2 "match"

\# Show byte offset with output lines
echo "text with pattern in it" | grep -b "pattern"

\# Suppress filename prefix on output
echo "content" > temp_file.txt
grep -h "content" temp_file.txt

\# Show filenames only (even with single file)
grep -H "content" temp_file.txt

\# Null-terminated output (useful for xargs -0)
grep -Z -l "pattern" temp_file.txt | tr '\\0' '\\n'

\# Colorize matches (if your grep supports it)
echo "text with pattern in it" | grep --color=always "pattern" || echo "Color not supported"

\# Quiet mode (exit status only, no output)
grep -q "pattern" temp_file.txt && echo "found" || echo "not found"

\# Cleanup
rm temp_file.txt
`,
  '016_grep_basic.sh': `\#!/bin/bash

\# Basic grep usage examples
\# Demonstrates fundamental grep operations

\# Basic usage
grep "pattern" /dev/null || echo "No matches found"

\# Case-insensitive search
echo "HELLO world" | grep -i "hello"

\# Invert match (lines NOT matching)
echo -e "line1\\nline2\\nline3" | grep -v "line2"

\# Show line numbers
echo -e "first\\nsecond\\nthird" | grep -n "second"

\# Count matching lines only
echo -e "match\\nno match\\nmatch again" | grep -c "match"

\# Only print the matching part of the line
echo "text with pattern123 in it" | grep -o "pattern[0-9]\\+"
`,
  '017_grep_context.sh': `\#!/bin/bash

\# Grep context and file operation examples
\# Demonstrates grep's context and file handling capabilities

\# Context lines: after, before, and both
echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -A 2 "TARGET"
echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -B 2 "TARGET"
echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -C 1 "TARGET"

\# Recursive search in current directory
echo "Creating test files..."
echo "pattern in file1" > temp_file1.txt
echo "no pattern in file2" > temp_file2.txt
echo "pattern in file3" > temp_file3.txt

echo "Recursive search results:"
grep -r "pattern" . --include="*.txt"

echo Result 2...
\# Print file names with matches
grep -l "pattern" *.txt | sort

echo Result 3...
\# Print file names without matches
grep -L "pattern" *.txt

\# Cleanup
rm temp_file*.txt
`,
  '018_grep_params.sh': `\#!/bin/bash

\# Grep parameters and options examples
\# Demonstrates various grep command line parameters

set -euo pipefail

echo "== Basic grep parameters =="
echo "text with pattern" | grep -i "PATTERN"
echo -e "line1\\nline2\\nline3" | grep -v "line2"
echo -e "match\\nno match\\nmatch again" | grep -c "match"

echo "== Context parameters =="
echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -A 2 "TARGET"
echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -B 2 "TARGET"
echo -e "line1\\nline2\\nTARGET\\nline4\\nline5" | grep -C 1 "TARGET"

echo "== File handling parameters =="
echo "content" > temp_file.txt
grep -H "content" temp_file.txt
grep -h "content" temp_file.txt
grep -l "content" temp_file.txt
grep -L "nonexistent" temp_file.txt || true

echo "== Output formatting parameters =="
echo "text with pattern in it" | grep -o "pattern"
echo "text with pattern in it" | grep -b "pattern"
echo "text with pattern in it" | grep -n "pattern"

echo "== Recursive and include/exclude parameters =="
mkdir -p test_dir
echo "pattern here" > test_dir/file1.txt
echo "no pattern" > test_dir/file2.txt
grep -r "pattern" test_dir
grep -r "pattern" test_dir --include="*.txt"
grep -r "pattern" test_dir --exclude="*.bak"
grep -r -c "pattern" test_dir --include="*.txt"
grep -r "pattern" test_dir --include="*.txt" | wc -l

echo "== Advanced parameters =="
echo -e "match1\\nmatch2\\nmatch3\\nmatch4" | grep -m 2 "match"
echo "text with pattern in it" | grep -q "pattern" && echo "found" || echo "not found"
grep -Z -l "pattern" temp_file.txt | tr '\\0' '\\n'

\# Cleanup
rm -f temp_file.txt
rm -rf test_dir
`,
  '019_grep_regex.sh': `\#!/bin/bash

\# Grep regex and pattern matching examples
\# Demonstrates advanced grep pattern capabilities

\# Extended regular expressions (ERE)
echo "foo123 bar456" | grep -E "(foo|bar)[0-9]+"

\# Fixed strings (no regex)
echo "a+b*c?" | grep -F "a+b*c?"

\# Match whole words
echo "word wordly subword" | grep -w "word"

\# Match whole lines
echo -e "exact whole line\\npartial line" | grep -x "exact whole line"

\# Multiple patterns
echo -e "error message\\nwarning message\\ninfo message" | grep -E "error|warning"

\# Read patterns from here-string
echo -e "error\\nwarning" | grep -f <(echo -e "error\\nwarning")

\# Complex regex with groups
echo "file123.txt backup456.bak" | grep -E "([a-z]+)([0-9]+)\\.([a-z]+)"
`,
  '020_ansi_quoting_basic.sh': `\#!/usr/bin/env bash

\# Basic ANSI-C quoting examples
set -euo pipefail

echo "== ANSI-C quoting =="
echo \$'line1\\nline2\\tTabbed'
`,
  '021_ansi_quoting_escape.sh': `\#!/usr/bin/env bash

\# Escape sequence examples
set -euo pipefail

echo "== Escape sequences =="
echo \$'bell\\a'
echo \$'backspace\\b'
echo \$'formfeed\\f'
echo \$'newline\\n'
echo \$'carriage\\rreturn'
echo \$'tab\\tseparated'
echo \$'vertical\\vtab'
`,
  '022_ansi_quoting_unicode.sh': `\#!/usr/bin/env bash

\# Unicode and hex examples
set -euo pipefail

echo "== Unicode and hex =="
echo \$'\\u0048\\u0065\\u006c\\u006c\\u006f'  \# Hello
echo \$'\\x48\\x65\\x6c\\x6c\\x6f'            \# Hello
`,
  '023_ansi_quoting_practical.sh': `\#!/usr/bin/env bash

\# Practical ANSI-C quoting examples
set -euo pipefail

echo "== Practical examples =="
\# Create a formatted table
printf \$'%-10s %-10s %s\\n' "Name" "Age" "City"
printf \$'%-10s %-10s %s\\n' "John" "25" "NYC"
printf \$'%-10s %-10s %s\\n' "Jane" "30" "LA"
`,
  '024_parameter_expansion_case.sh': `\#!/usr/bin/env bash

\# Case modification in parameter expansion
set -euo pipefail

echo "== Case modification in parameter expansion =="
name="world"
echo "\${name^^}"        \# WORLD
echo "\${name,,}"        \# world
echo "\${name^}"         \# World
`,
  '025_parameter_expansion_advanced.sh': `\#!/usr/bin/env bash

\# Advanced parameter expansion examples
set -euo pipefail

echo "== Advanced parameter expansion =="
path="/tmp/file.txt"
echo "\${path\#\#*/}"       \# file.txt
echo "\${path%/*}"        \# /tmp
s2="abba"; echo "\${s2//b/X}"  \# aXXa
`,
  '026_parameter_expansion_more.sh': `\#!/usr/bin/env bash

\# More parameter expansion examples
set -euo pipefail

echo "== More parameter expansion =="
var="hello world"
echo "\${var\#hello}"      \#  world
echo "\${var%world}"      \# hello 
echo "\${var//o/0}"       \# hell0 w0rld
`,
  '027_parameter_expansion_defaults.sh': `\#!/usr/bin/env bash

\# Default values in parameter expansion
set -euo pipefail

echo "== Default values =="
unset maybe
echo "\${maybe:-default}"  \# default
echo "\${maybe:=default}"  \# default (and sets maybe)
echo "\${maybe:?error}"    \# error if unset
`,
  '028_arrays_indexed.sh': `\#!/usr/bin/env bash

\# Indexed array examples
set -euo pipefail

echo "== Indexed arrays =="
arr=(one two three )
echo "\${arr[1]}"        \# two
echo "\${\#arr[@]}"       \# 3
for x in "\${arr[@]}"; do printf "%s " "\$x"; done; echo
`,
  '029_arrays_associative.sh': `\#!/usr/bin/env bash

\# Associative array examples
set -euo pipefail

echo "== Associative arrays =="
declare -A map
map[foo]=bar
map[answer]=42
map[two]="1 + 1"
echo "\${map[foo]}"      \# bar
echo "\${map[answer]}"   \# 42

\# Show all keys and values
for k in "\${!map[@]}"; do echo "\$k => \${map[\$k]}"; done | sort
`,
  '030_control_flow_if.sh': `\#!/bin/bash

\# If statement examples
if [ -f "file.txt" ]; then
    echo "File exists"
else
    echo "File does not exist"
fi
`,
  '031_control_flow_loops.sh': `\#!/bin/bash

\# Loop examples
for i in {1..5}; do
    echo "Number: \$i"
done

for i in {1..3}; do j=\$((j+1)); done; echo \$j

while [ \$i -lt 10 ]; do
    echo "Counter: \$i"
    i=\$((i + 1))
done
`,
  '032_control_flow_function.sh': `\#!/bin/bash

\# Function examples
function greet() {
    echo "Hello, \$1!"
}

greet "World"
`,
  '033_brace_expansion_basic.sh': `\#!/usr/bin/env bash

\# Basic brace expansion examples
set -euo pipefail

echo "== Basic brace expansion =="
echo {1..5}
echo {a..c}
echo {00..04..2}
`,
  '034_brace_expansion_advanced.sh': `\#!/usr/bin/env bash

\# Advanced brace expansion examples
set -euo pipefail

echo "== Advanced brace expansion =="
echo {a,b,c}{1,2,3}
echo {1..10..2}
echo {a..z..3}
`,
  '035_brace_expansion_practical.sh': `\#!/usr/bin/env bash

\# Practical brace expansion examples
set -euo pipefail

echo "== Practical examples =="
\# Create numbered files
touch file_{001..005}.txt
ls file_*.txt
rm file_*.txt
`,
  '036_pattern_matching_basic.sh': `\#!/usr/bin/env bash

\# Basic pattern matching examples
set -euo pipefail

echo "== [[ pattern and regex ]]"
s="file.txt"
[[ \$s == *.txt ]] && echo pattern-match
[[ \$s =~ ^file\\.[a-z]+\$ ]] && echo regex-match
`,
  '037_pattern_matching_extglob.sh': `\#!/usr/bin/env bash

\# Extended glob examples
set -euo pipefail

echo "== extglob =="
shopt -s extglob
f1="file.js"; f2="thing.min.js"
[[ \$f1 == !(*.min).js ]] && echo f1-ok
[[ \$f2 == !(*.min).js ]] || echo f2-filtered
`,
  '038_pattern_matching_nocase.sh': `\#!/usr/bin/env bash

\# Case-insensitive matching examples
set -euo pipefail

echo "== nocasematch =="
shopt -s nocasematch
word="Foo"; [[ \$word == foo ]] && echo ci-match
`,
  '039_process_substitution_here.sh': `\#!/usr/bin/env bash

\# Here-string examples
set -euo pipefail

echo "== Here-string with grep -o =="
grep -o pattern <<< "some pattern here"
`,
  '040_process_substitution_comm.sh': `\#!/usr/bin/env bash

\# Process substitution with comm examples
set -euo pipefail

echo "== Process substitution with comm =="
comm -12 <(printf 'a\\nb\\n') <(printf 'b\\nc\\n')
`,
  '041_process_substitution_mapfile.sh': `\#!/usr/bin/env bash

\# mapfile examples
set -euo pipefail

echo "== readarray/mapfile =="
mapfile -t lines < <(printf 'x\\ny\\n')
printf '%s ' "\${lines[@]}"; echo
`,
  '042_process_substitution_advanced.sh': `\#!/usr/bin/env bash

\# Advanced process substitution examples
set -euo pipefail

echo "== More process substitution examples =="
\# Compare sorted outputs
diff <(echo -e "a\\nc\\nb" | sort) <(echo -e "a\\nb\\nd" | sort) || echo "Files differ"

\# Use paste with process substitution
paste <(echo -e "name1\\nname2") <(echo -e "value1\\nvalue2")
`,
  '043_home.sh': `[ ~ = "\$HOME" ] && echo 1 || echo -
[ ~/Documents = "\$HOME" ] && echo 2 || echo -
[ ~/Documents = "\$HOME/Documents" ] && echo 3 || echo -`,
  '044_find_example.sh': `\#!/bin/bash

\# Find all .txt files in current directory and subdirectories
echo '\#find . -name "*.txt" -type f | sort'
find . -name "*.txt" -type f | sort

\# Find files modified in the last 7 days
echo '
find . -mtime -7 -type f  | sort'
find . -mtime -7 -type f  | sort

\# Find files modified in the last 1 day
echo '
find . -mtime -1 -type f  | sort'
find . -mtime -1 -type f  | sort

\# Find files modified in the last 1 hour
\# dd
echo '
find . -mmin -60 -type f  | sort'
find . -mmin -60 -type f  | sort

\# Find files larger than 1MB
echo '
find . -size +1M -type f  | sort'
find . -size +1M -type f  | sort

\# Find empty files and directories
echo '
find . -empty  | sort'
find . -empty  | sort

\# Don't use  yet, they are not portable
\# Find files with specific permissions (executable)
\# find . -perm -u+x -type f

\# Find files by owner
\#find . -user \$USER -type f

\# Find files by group
\#find . -group \$(id -gn) -type f

\# Find files and execute command on them
echo 'touch/ls/rm'
touch a.logtmp a.logtmp.sav
find . -name "*.logtmp" -exec rm {} \\;

ls *.logtmp*

rm a.logtmp.sav

\# Find files and show detailed information
\#echo 'find . -type f -ls  | sort'
\#find . -type f -ls  | sort

\# Find files excluding certain directories
echo 'find .. -type f -not -path "./.git/*" -not -path "./node_modules/*"  | sort'
find .. -type f -not -path "./.git/*" -not -path "./node_modules/*"  | sort
`,
  '045_shell_calling_perl.sh': `\#!/bin/bash

echo Warmup 1
echo "apple" | perl -ne 'print "Fruit: \$_\\n"'

\#echo Warmup 2
\#perl -e "print \\"Shell variable: \$ENV{SHELL_VAR}\\n\\""


\# Example 1: Simple Perl one-liner to print text
echo "=== Example 1: Simple Perl one-liner ==="
perl -e 'print "Hello from Perl!\\n"'

\# Example 2: Perl script with command line arguments
echo -e "\\n=== Example 2: Perl with arguments ==="
perl -e 'foreach \$arg (@ARGV) { print "Argument: \$arg\\n" }' "first" "second" "third"

\# Example 3: Perl script processing shell variables
echo -e "\\n=== Example 3: Perl processing shell variables ==="
SHELL_VAR="Hello World"
perl -e 'print "Shell variable: \$ENV{SHELL_VAR}\\n"'
export SHELL_VAR 
perl -e 'print "Shell variable: \$ENV{SHELL_VAR}\\n"'

\# Example 4: Perl script reading from shell pipeline
echo -e "\\n=== Example 4: Perl reading from pipeline ==="
echo -e "apple\\nbanana\\ncherry" | perl -ne 'chomp; print "Fruit: \$_\\n"'

\# Example 5: Complex Perl script with here document
echo -e "\\n=== Example 5: Perl script with here document ==="
perl << 'EOF'
use strict;
use warnings;

my @numbers = (1, 2, 3, 4, 5);
my \$sum = 0;

foreach my \$num (@numbers) {
    \$sum += \$num;
    print "Added \$num, sum is now \$sum\\n";
}

print "Final sum: \$sum\\n";
EOF

`,
  '046_cd..sh': `cd ..
ls
`,
  '047_for_arithematic.sh': `for i in {1..5}
do
	j=\$((\$j*\$i))
done
echo \$j
`,
  '048_subprocess.sh': `(sleep 1; echo a)&
echo b`,
  '049_local.sh': `a=1
echo \$a
(a=2; echo \$a)
(echo \$a)
echo \$a`,
  '050_test_ls_star_dot_sh.sh': `\#!/usr/bin/env bash
set -euo pipefail

echo "Testing ls * .sh:"
ls * .sh
`,
  '051_primes.sh': `\#!/bin/bash

\# Prime Number Generator
\# This script finds the first 1000 prime numbers

\#If the parser doesn't support += let it choke on this easy examples.
y+=2
z+=(a b)
z+=\${primes[@]:0:1}

echo "=== Prime Number Generator (first 1000 primes) ==="

\# Function to check if a number is prime
is_prime() {
    local n=\$1
    
    if [ \$n -lt 2 ]; then
        return 1
    fi
    
    if [ \$n -eq 2 ]; then
        return 0
    fi
    
    if [ \$((n % 2)) -eq 0 ]; then
        return 1
    fi
    
    local sqrt_n=\$(echo "sqrt(\$n)" | bc)
    local i=3
    
    while [ \$i -le \$sqrt_n ]; do
        if [ \$((n % i)) -eq 0 ]; then
            return 1
        fi
        i=\$((i + 2))
    done
    
    return 0
}

echo "Finding first 100 prime numbers..."
echo "This may take a while..."

primes=(2)
count=1
candidate=3

while [ \$count -lt 100 ]; do
    if is_prime \$candidate; then
        primes+=(\$candidate)
        count=\$((count + 1))
        
        \# Show progress every 10 primes
        if [ \$((count % 10)) -eq 0 ]; then
            echo "Found \$count primes so far..."
        fi
    fi
    candidate=\$((candidate + 2))
done

echo ""
echo "First 1000 prime numbers found!"
echo "Count: \${\#primes[@]}"
echo "First 10: \${primes[@]:0:10}"
echo "Last 10: \${primes[@]: -10}"

echo "Prime number generation complete!"
`,
  '052_numeric_computations.sh': `\#!/bin/bash

\# Comprehensive Numeric Computation Examples
\# This script demonstrates various mathematical algorithms

echo "=== Comprehensive Numeric Computation Examples ==="
echo ""

\# Function to calculate Fibonacci numbers
fibonacci() {
    local n=\$1
    local a=0
    local b=1
    
    if [ \$n -le 1 ]; then
        echo \$n
        return
    fi
    
    for ((i=2; i<=n; i++)); do
        local temp=\$((a + b))
        a=\$b
        b=\$temp
    done
    
    echo \$b
}

\# Function to factorize a number
factorize() {
    local n=\$1
    local divisor=2
    local factors=""
    
    echo -n "Factors of \$n: "
    
    while [ \$n -gt 1 ]; do
        while [ \$((n % divisor)) -eq 0 ]; do
            if [ -z "\$factors" ]; then
                factors="\$divisor"
            else
                factors="\$factors * \$divisor"
            fi
            n=\$((n / divisor))
        done
        divisor=\$((divisor + 1))
        
        \# Optimization: stop if divisor^2 > n
        if [ \$((divisor * divisor)) -gt \$n ]; then
            if [ \$n -gt 1 ]; then
                if [ -z "\$factors" ]; then
                    factors="\$n"
                else
                    factors="\$factors * \$n"
                fi
            fi
            break
        fi
    done
    
    echo "\$factors"
}

\# Function to check if a number is prime
is_prime() {
    local n=\$1
    
    if [ \$n -lt 2 ]; then
        return 1
    fi
    
    if [ \$n -eq 2 ]; then
        return 0
    fi
    
    if [ \$((n % 2)) -eq 0 ]; then
        return 1
    fi
    
    local sqrt_n=\$(echo "sqrt(\$n)" | bc)
    local i=3
    
    while [ \$i -le \$sqrt_n ]; do
        if [ \$((n % i)) -eq 0 ]; then
            return 1
        fi
        i=\$((i + 2))
    done
    
    return 0
}

\# Function to find first N primes
find_primes() {
    local count=\$1
    local primes=(2)
    local found=1
    local candidate=3
    
    echo "Finding first \$count prime numbers..."
    
    while [ \$found -lt \$count ]; do
        if is_prime \$candidate; then
            primes+=(\$candidate)
            found=\$((found + 1))
            
            \# Show progress every 100 primes
            if [ \$((found % 100)) -eq 0 ]; then
                echo "Found \$found primes so far..."
            fi
        fi
        candidate=\$((candidate + 2))
    done
    
    echo "First \$count primes found!"
    echo "First 10: \${primes[@]:0:10}"
    echo "Last 10: \${primes[@]: -10}"
}

\# Function to calculate GCD
gcd() {
    local a=\$1
    local b=\$2
    
    while [ \$b -ne 0 ]; do
        local temp=\$b
        b=\$((a % b))
        a=\$temp
    done
    
    echo \$a
}

\# Function to calculate LCM (Least Common Multiple)
lcm() {
    local a=\$1
    local b=\$2
    local gcd_result=\$(gcd \$a \$b)
    echo \$((a * b / gcd_result))
}

\# Performance measurement function
measure_time() {
    local start_time=\$(date +%s%N)
    eval "\$1"
    local end_time=\$(date +%s%N)
    local duration=\$((end_time - start_time))
    echo "Duration: \$((duration / 1000000)) ms"
}

echo "1. Fibonacci Sequence (first 20 numbers):"
fib_numbers=""
for i in {0..19}; do
    fib_numbers="\$fib_numbers \$(fibonacci \$i)"
done
echo "   \$fib_numbers"
echo ""

echo "2. Number Factorization:"
factorize 12
factorize 28
factorize 100
factorize 12345
echo ""

echo "3. Prime Number Generation:"
find_primes 100  \# Reduced to 100 for faster execution
echo ""

echo "4. Greatest Common Divisor Examples:"
echo "   gcd(48, 18) = \$(gcd 48 18)"
echo "   gcd(54, 24) = \$(gcd 54 24)"
echo "   gcd(7, 13) = \$(gcd 7 13)"
echo "   gcd(100, 25) = \$(gcd 100 25)"
echo "   gcd(12345, 67890) = \$(gcd 12345 67890)"
echo ""

echo "5. Least Common Multiple Examples:"
echo "   lcm(12, 18) = \$(lcm 12 18)"
echo "   lcm(15, 20) = \$(lcm 15 20)"
echo "   lcm(8, 12) = \$(lcm 8 12)"
echo ""

echo "6. Performance Benchmarks:"
echo "   Computing fibonacci(30):"
measure_time "fibonacci 30 > /dev/null"

echo "   Factorizing 12345:"
measure_time "factorize 12345 > /dev/null"

echo "   Finding first 50 primes:"
measure_time "find_primes 50 > /dev/null"

echo ""
echo "=== All numeric computations complete! ==="
echo ""
echo "You can now test these scripts with your translator:"
echo "  ./sh2perl parse --perl examples/fibonacci.sh"
echo "  ./sh2perl parse --rust examples/factorize.sh"
echo "  ./sh2perl parse --python examples/primes.sh"
echo "  ./sh2perl parse --lua examples/gcd.sh"
echo "  ./sh2perl parse --js examples/numeric_computations.sh"
`,
  '053_gcd.sh': `\#!/bin/bash

\# Greatest Common Divisor Calculator
\# This script calculates GCD using Euclidean algorithm

echo "=== Greatest Common Divisor Examples ==="

\# Function to calculate GCD using Euclidean algorithm
gcd() {
    local a=\$1
    local b=\$2
    
    while [ \$b -ne 0 ]; do
        local temp=\$b
        b=\$((a % b))
        a=\$temp
    done
    
    echo \$a
}

\# Test with various number pairs
echo "GCD calculations:"

\# Test case 1: 48 and 18
result=\$(gcd 48 18)
echo "gcd(48, 18) = \$result"

\# Test case 2: 54 and 24
result=\$(gcd 54 24)
echo "gcd(54, 24) = \$result"

\# Test case 3: 7 and 13 (coprime)
result=\$(gcd 7 13)
echo "gcd(7, 13) = \$result"

\# Test case 4: 100 and 25
result=\$(gcd 100 25)
echo "gcd(100, 25) = \$result"

\# Test case 5: 12345 and 67890
result=\$(gcd 12345 67890)
echo "gcd(12345, 67890) = \$result"

\# Interactive mode
echo ""
echo "Enter two numbers to calculate their GCD (or press Ctrl+C to exit):"
while true; do
    echo -n "First number: "
    read -r num1
    
    if [ -z "\$num1" ]; then
        break
    fi
    
    echo -n "Second number: "
    read -r num2
    
    if [ -z "\$num2" ]; then
        break
    fi
    
    if [[ "\$num1" =~ ^[0-9]+\$ ]] && [[ "\$num2" =~ ^[0-9]+\$ ]]; then
        result=\$(gcd \$num1 \$num2)
        echo "gcd(\$num1, \$num2) = \$result"
    else
        echo "Please enter valid positive integers."
    fi
    
    echo ""
done

echo "GCD calculation complete!"
`,
  '054_fibonacci.sh': `\#!/bin/bash

\# Fibonacci Sequence Calculator
\# This script calculates and displays the first 20 Fibonacci numbers

echo "=== Fibonacci Sequence (first 20 numbers) ==="

\# Initialize first two numbers
a=0
b=1

echo "Fibonacci numbers:"
echo -n "\$a \$b "

\# Calculate next 18 numbers
for i in {3..20}; do
    temp=\$((a + b))
    echo -n "\$temp "
    a=\$b
    b=\$temp
done

echo ""
echo "Done!"
`,
  '055_factorize.sh': `\#!/bin/bash

\# Number Factorization Calculator
\# This script finds the prime factors of given numbers

echo "=== Number Factorization Examples ==="

\# Function to factorize a number
factorize() {
    local n=\$1
    local divisor=2
    local factors=""
    
    echo -n "Factors of \$n: "
    
    while [ \$n -gt 1 ]; do
        while [ \$((n % divisor)) -eq 0 ]; do
            if [ -z "\$factors" ]; then
                factors="\$divisor"
            else
                factors="\$factors * \$divisor"
            fi
            n=\$((n / divisor))
        done
        divisor=\$((divisor + 1))
        
        \# Optimization: stop if divisor^2 > n
        if [ \$((divisor * divisor)) -gt \$n ]; then
            if [ \$n -gt 1 ]; then
                if [ -z "\$factors" ]; then
                    factors="\$n"
                else
                    factors="\$factors * \$n"
                fi
            fi
            break
        fi
    done
    
    echo "\$factors"
}

\# Test with various numbers
factorize 12
factorize 28
factorize 100
factorize 12345

echo "Factorization complete!"
`,
  '056_send_args.sh': `bash examples/005_args.sh one
bash examples/005_args.sh one two
bash examples/005_args.sh one two three
bash examples/005_args.sh 1
bash examples/005_args.sh 1 2 3
bash examples/005_args.sh 1 two 3
bash examples/005_args.sh "A 'quoted' Sting"
bash examples/005_args.sh "A 'quoted' Sting" 2 3 4 5 6


`,
  '057_case.sh': `\#!/bin/bash

\# Case statement examples
\# This demonstrates the bash case statement syntax and common usage patterns

echo "=== Basic Case Statement Example ==="

\# Example 1: Basic case statement with simple patterns
case "\$1" in
    "start")
        echo "Starting the service..."
        ;;
    "stop")
        echo "Stopping the service..."
        ;;
    "restart")
        echo "Restarting the service..."
        ;;
    *)
        echo "Usage: \$0 {start|stop|restart}"
        exit 1
        ;;
esac

echo "=== Case Statement with Pattern Matching ==="

\# Example 2: Case statement with pattern matching
filename="\$2"
case "\$filename" in
    *.txt)
        echo "Processing text file: \$filename"
        ;;
    *.sh)
        echo "Processing shell script: \$filename"
        ;;
    *.py)
        echo "Processing Python file: \$filename"
        ;;
    *)
        echo "Unknown file type: \$filename"
        ;;
esac

echo "=== Case Statement with Multiple Patterns ==="

\# Example 3: Case statement with multiple patterns per case
case "\$3" in
    "help"|"h"|"-h"|"--help")
        echo "Help information:"
        echo "  start  - Start the service"
        echo "  stop   - Stop the service"
        echo "  status - Show service status"
        ;;
    "status"|"s"|"-s"|"--status")
        echo "Service status: Running"
        ;;
    *)
        echo "Unknown option: \$3"
        ;;
esac

echo "=== Case Statement with Character Classes ==="

\# Example 4: Case statement with character classes
case "\$4" in
    [0-9])
        echo "Single digit: \$4"
        ;;
    [a-z])
        echo "Lowercase letter: \$4"
        ;;
    [A-Z])
        echo "Uppercase letter: \$4"
        ;;
    [0-9][0-9])
        echo "Two digit number: \$4"
        ;;
    *)
        echo "Other character: \$4"
        ;;
esac

echo "=== Case Statement with Default Action ==="

\# Example 5: Case statement with default action
case "\$5" in
    "red")
        echo "Color is red"
        ;;
    "green")
        echo "Color is green"
        ;;
    "blue")
        echo "Color is blue"
        ;;
esac

echo "=== Case Statement with Commands ==="

\# Example 6: Case statement with command execution
case "\$6" in
    "ls")
        ls -la
        ;;
    "date")
        date
        ;;
    "pwd")
        pwd
        ;;
    "whoami")
        whoami
        ;;
    *)
        echo "Available commands: ls, date, pwd, whoami"
        ;;
esac
`,
  '058_advanced_bash_idioms.sh': `\#!/bin/bash

\# Advanced Bash Idioms: Nesting and Combining Control Blocks
\# This file demonstrates complex bash patterns and idioms

echo "=== Advanced Bash Idioms Examples ==="
echo

\# Example 1: Nested loops with conditional logic and array manipulation
echo "1. Nested loops with conditional logic and array manipulation:"
numbers=(1 2 3 4 5)
letters=(a b c d e)
for num in "\${numbers[@]}"; do
    for letter in "\${letters[@]}"; do
        if [[ \$num -gt 3 && \$letter != "c" ]]; then
            echo "  Number \$num with letter \$letter (filtered)"
        fi
    done
done
echo

\# Example 2: Function with nested case statements and parameter expansion
echo "2. Function with nested case statements and parameter expansion:"
process_data() {
    local data_type="\$1"
    local value="\$2"
    
    case "\$data_type" in
        "string")
            case "\${value,,}" in  \# Convert to lowercase
                "hello"|"hi")
                    echo "  Greeting detected: \$value"
                    ;;
                "bye"|"goodbye")
                    echo "  Farewell detected: \$value"
                    ;;
                *)
                    echo "  Unknown string: \$value"
                    ;;
            esac
            ;;
        "number")
            if [[ "\$value" =~ ^[0-9]+\$ ]]; then
                if (( value % 2 == 0 )); then
                    echo "  Even number: \$value"
                else
                    echo "  Odd number: \$value"
                fi
            else
                echo "  Invalid number: \$value"
            fi
            ;;
        *)
            echo "  Unknown data type: \$data_type"
            ;;
    esac
}

process_data "string" "Hello"
process_data "string" "Bye"
process_data "number" "42"
process_data "number" "17"
echo

\# Example 3: Complex conditional with command substitution and arithmetic
echo "3. Complex conditional with command substitution and arithmetic:"
file_count=\$(find . -maxdepth 1 -type f | wc -l)
dir_count=\$(find . -maxdepth 1 -type d | wc -l)

if [[ \$file_count -gt 0 && \$dir_count -gt 1 ]]; then
    if (( file_count > dir_count )); then
        echo "  More files (\$file_count) than directories (\$dir_count)"
    elif (( file_count == dir_count )); then
        echo "  Equal count: \$file_count files and \$dir_count directories"
    else
        echo "  More directories (\$dir_count) than files (\$file_count)"
    fi
else
    echo "  Insufficient items for comparison"
fi
echo

\# Example 4: Nested here-documents with parameter expansion
echo "4. Nested here-documents with parameter expansion:"
user="admin"
host="localhost"
port="22"

cat <<-EOF
    SSH Configuration:
    \$(cat <<-INNER
        User: \$user
        Host: \$host
        Port: \$port
        Status: \$(ping -c 1 \$host >/dev/null 2>&1 && echo "Online" || echo "Offline")
    INNER
    )
EOF
echo

\# Example 5: Array processing with nested loops and conditional logic
echo "5. Array processing with nested loops and conditional logic:"
declare -A matrix
matrix[0,0]=1; matrix[0,1]=2; matrix[0,2]=3
matrix[1,0]=4; matrix[1,1]=5; matrix[1,2]=6
matrix[2,0]=7; matrix[2,1]=8; matrix[2,2]=9

for i in {0..2}; do
    for j in {0..2}; do
        value=\${matrix[\$i,\$j]}
        if [[ \$value -gt 5 ]]; then
            echo -n "  [\$value] "
        else
            echo -n "  \$value "
        fi
    done
    echo
done
echo

\# Example 6: Process substitution with nested commands and error handling
echo "6. Process substitution with nested commands and error handling:"
{


echo "  First word: \${test_string%% *}"
echo "  Last word: \${test_string\#\#* }"
echo "  Middle: \${test_string\#* }"
echo "  Middle: \${test_string% *}"
echo "  Uppercase: \${test_string^^}"
echo "  Lowercase: \${test_string,,}"
echo "  Capitalize: \${test_string^}"
echo

\# Example 11: Complex arithmetic with nested expressions
echo "11. Complex arithmetic with nested expressions:"
a=10
b=5
c=3

result=\$(( (a + b) * c - (a % b) / c ))
echo "  Expression: (a + b) * c - (a % b) / c"
echo "  Values: a=\$a, b=\$b, c=\$c"
echo "  Result: \$result"

\# Nested arithmetic in conditional
if (( (a > b) && (b < c) || (a % 2 == 0) )); then
    echo "  Complex condition met: a > b AND (b < c OR a is even)"
fi
echo

\# Example 12: Nested command substitution with error handling
echo "12. Nested command substitution with error handling:"
echo "  Current directory: \$(pwd)"
echo "  Parent directory: \$(dirname "\$(pwd)")"
echo "  Home directory: \$(dirname "\$(dirname "\$(pwd)")")"

\# Nested command with fallback
file_info=\$(stat -c "%s %y" "nonexistent_file" 2>/dev/null || echo "File not found")
echo "  File info: \$file_info"
echo

echo "=== Advanced Bash Idioms Examples Complete ==="
`,
  '059_issue3.sh': `if [ \$\# -lt 2 ]; then
    echo "One"
    echo "Two"
fi
`,
  '060_issue5.sh': `labelargs="foo"
`,
  '061_test_local_names_preserved.sh': `\#!/bin/bash

function test_math() {
    local first_number=\$1
    local second_number=\$2
    local operation=\$3
    
    case \$operation in
        "add")
            echo \$((first_number + second_number))
            ;;
        "subtract")
            echo \$((first_number - second_number))
            ;;
        "multiply")
            echo \$((first_number * second_number))
            ;;
        *)
            echo "Unknown operation: \$operation"
            ;;
    esac
}

function test_strings() {
    local input_string=\$1
    local search_pattern=\$2
    local replacement=\$3
    
    case \$search_pattern in
        "start")
            echo "Replacing start of: \$input_string with: \$replacement"
            ;;
        "end")
            echo "Replacing end of: \$input_string with: \$replacement"
            ;;
        "middle")
            echo "Replacing middle of: \$input_string with: \$replacement"
            ;;
        *)
            echo "Unknown pattern: \$search_pattern for string: \$input_string"
            ;;
    esac
}

function test_arrays() {
    local array_name=\$1
    local index=\$2
    local new_value=\$3
    
    case \$index in
        "first")
            echo "Setting first element of \$array_name to \$new_value"
            ;;
        "last")
            echo "Setting last element of \$array_name to \$new_value"
            ;;
        *)
            echo "Setting element \$index of \$array_name to \$new_value"
            ;;
    esac
}

\# Test math function with meaningful local variable names
test_math 10 5 "add"
test_math 10 5 "multiply"

\# Test string function with meaningful local variable names
test_strings "hello world" "start" "hi"
test_strings "hello world" "end" "bye"

\# Test array function with meaningful local variable names
test_arrays "my_array" "first" "new_value"
test_arrays "my_array" "last" "final_value"
`,
  '062_01_ambiguous_operators.sh': `\#!/bin/bash

\# 1. Ambiguous operators and precedence issues
\# The lexer needs to handle these correctly with proper priorities
echo "Testing ambiguous operators..."
result=\$((2**3**2))  \# Should be 2**(3**2) = 2^9 = 512, not (2^3)^2 = 64
echo "2**3**2 = \$result"
`,
  '062_02_complex_parameter_expansions.sh': `\#!/bin/bash

\# 2. Complex nested parameter expansions with conflicting delimiters
echo "Testing complex parameter expansions..."
complex_var="hello world"
echo "\${complex_var\#*o}"  \# Remove shortest match from beginning
echo "\${complex_var\#\#*o}" \# Remove longest match from beginning
echo "\${complex_var%o*}"  \# Remove shortest match from end
echo "\${complex_var%%o*}" \# Remove longest match from end
`,
  '062_03_complex_heredocs.sh': `\#!/bin/bash

\# 3. Here-documents with complex delimiters and nested structures
echo "Testing complex here-documents..."
cat <<'EOF'
This is a test line
This is not a test line
This is another test line
EOF
`,
  '062_04_nested_arithmetic.sh': `\#!/bin/bash

\# 4. Nested arithmetic expressions with conflicting parentheses
echo "Testing nested arithmetic..."
result=\$(( (2 + 3) * (4 - 1) + (5 ** 2) ))
echo "Complex arithmetic: \$result"
`,
  '062_05_nested_command_substitution.sh': `\#!/bin/bash

\# 5. Command substitution within parameter expansion
echo "Testing nested command substitution..."
echo "Current dir: \${PWD:-\$(pwd)}" | tr -d '/\\\\' | grep -o '.....\$' \#ignore differences between WSL and Windows
\#echo "User: \${USER:-\$(whoami)}"
`,
  '062_06_process_substitution.sh': `\#!/bin/bash

\# 6. Process substitution with complex commands
echo "Testing process substitution..."
\# diff <(sort file1.txt) <(sort file2.txt)  \# Commented out as files don't exist
`,
  '062_07_complex_brace_expansion.sh': `\#!/bin/bash

\# 7. Brace expansion with nested patterns
echo "Testing complex brace expansion..."
echo {a,b,c}{1,2,3}{x,y,z}
`,
  '062_08_simple_case.sh': `\#!/bin/bash

\# 8. Simple case statement to avoid parser issues
echo "Testing simple case patterns..."
case "\$1" in
    "test")
        echo "Matched test"
        ;;
    *)
        echo "Default case"
        ;;
esac
`,
  '062_09_complex_function.sh': `\#!/bin/bash

\# 9. Function with complex parameter handling
function complex_function() {
    local param1="\$1"
    local param2="\${2:-default}"
    local param3="\${3//\\"/\\\\\\"}"  \# Replace quotes with escaped quotes
    
    echo "Param1: \$param1"
    echo "Param2: \$param2"
    echo "Param3: \$param3"
    
    \# Nested command substitution
    local result=\$(echo "\$param1" | sed "s/old/new/g")
    echo "Result: \$result"
}

\# Test the complex function
complex_function "test\\"quote" "second_param" "third\\"param"
`,
  '062_10_simple_pipeline.sh': `\#!/bin/bash

\# 10. Simple pipeline without complex redirections
echo "Testing simple pipeline..."
ls -la | grep "^d" | head -5
`,
  '062_11_mixed_arithmetic.sh': `\#!/bin/bash

\# 11. Arithmetic with mixed bases and complex expressions
echo "Testing mixed arithmetic..."
hex=255
octal=511
binary=10
result=\$(( hex + octal + binary ))
echo "Mixed base result: \$result"
`,
  '062_12_complex_string_interpolation.sh': `\#!/bin/bash

\# 12. Complex string interpolation with nested expansions
echo "Testing complex string interpolation..."
message="Hello, \${USER:-\$(whoami)}! Your home is \${HOME:-\$(echo ~)}"
echo "\$message"
`,
  '062_13_simple_test_expressions.sh': `\#!/bin/bash

\# 13. Simple test expressions to avoid parser issues
echo "Testing simple test expressions..."
if [[ -f "file.txt" ]]; then
    echo "File exists"
else
    echo "File does not exist"
fi
`,
  '062_14_complex_array_operations.sh': `\#!/bin/bash

\# 14. Complex array operations
echo "Testing complex array operations..."
declare -a array=("item1" "item2" "item3")
array+=("item4")
echo "Array: \${array[@]}"
echo "Length: \${\#array[@]}"
echo "First item: \${array[0]}"
`,
  '062_15_complex_local_variables.sh': `\#!/bin/bash

\# 15. Function with complex local variable declarations
function test_locals() {
    local var1="\$1"
    local var2="\${2:-default_value}"
    local var3="\$(echo "\$var1" | tr '[:lower:]' '[:upper:]')"
    
    echo "Var1: \$var1"
    echo "Var2: \$var2"
    echo "Var3: \$var3"
}

\# Test the function
test_locals "hello" "world"
`,
  '062_hard_to_lex.sh': `\#!/bin/bash

\# This script tests challenging lexing scenarios that can cause ambiguity
\# and parsing difficulties in shell lexers

\# 1. Ambiguous operators and precedence issues
\# The lexer needs to handle these correctly with proper priorities
echo "Testing ambiguous operators..."
result=\$((2**3**2))  \# Should be 2**(3**2) = 2^9 = 512, not (2^3)^2 = 64
echo "2**3**2 = \$result"

\# 2. Complex nested parameter expansions with conflicting delimiters
echo "Testing complex parameter expansions..."
complex_var="hello world"
echo "\${complex_var\#*o}"  \# Remove shortest match from beginning
echo "\${complex_var\#\#*o}" \# Remove longest match from beginning
echo "\${complex_var%o*}"  \# Remove shortest match from end
echo "\${complex_var%%o*}" \# Remove longest match from end

\# 3. Here-documents with complex delimiters and nested structures
echo "Testing complex here-documents..."
cat <<'EOF'
This is a test line
This is not a test line
This is another test line
EOF

\# 4. Nested arithmetic expressions with conflicting parentheses
echo "Testing nested arithmetic..."
result=\$(( (2 + 3) * (4 - 1) + (5 ** 2) ))
echo "Complex arithmetic: \$result"

\# 5. Command substitution within parameter expansion
echo "Testing nested command substitution..."
echo "Current dir: \${PWD:-\$(pwd)}"
echo "User: \${USER:-\$(whoami)}"

\# 6. Process substitution with complex commands
echo "Testing process substitution..."
\# diff <(sort file1.txt) <(sort file2.txt)  \# Commented out as files don't exist

\# 7. Brace expansion with nested patterns
echo "Testing complex brace expansion..."
echo {a,b,c}{1,2,3}{x,y,z}

\# 8. Simple case statement to avoid parser issues
echo "Testing simple case patterns..."
case "\$1" in
    "test")
        echo "Matched test"
        ;;
    *)
        echo "Default case"
        ;;
esac

\# 9. Function with complex parameter handling
function complex_function() {
    local param1="\$1"
    local param2="\${2:-default}"
    local param3="\${3//\\"/\\\\\\"}"  \# Replace quotes with escaped quotes
    
    echo "Param1: \$param1"
    echo "Param2: \$param2"
    echo "Param3: \$param3"
    
    \# Nested command substitution
    local result=\$(echo "\$param1" | sed "s/old/new/g")
    echo "Result: \$result"
}

\# 10. Simple pipeline without complex redirections
echo "Testing simple pipeline..."
ls -la | grep "^d" | head -5

\# 11. Arithmetic with mixed bases and complex expressions
echo "Testing mixed arithmetic..."
hex=255
octal=511
binary=10
result=\$(( hex + octal + binary ))
echo "Mixed base result: \$result"

\# 12. Complex string interpolation with nested expansions
echo "Testing complex string interpolation..."
message="Hello, \${USER:-\$(whoami)}! Your home is \${HOME:-\$(echo ~)}"
echo "\$message"

\# 13. Simple test expressions to avoid parser issues
echo "Testing simple test expressions..."
if [[ -f "file.txt" ]]; then
    echo "File exists"
else
    echo "File does not exist"
fi

\# 14. Complex array operations
echo "Testing complex array operations..."
declare -a array=("item1" "item2" "item3")
array+=("item4")
echo "Array: \${array[@]}"
echo "Length: \${\#array[@]}"
echo "First item: \${array[0]}"

\# 15. Function with complex local variable declarations
function test_locals() {
    local var1="\$1"
    local var2="\${2:-default_value}"
    local var3="\$(echo "\$var1" | tr '[:lower:]' '[:upper:]')"
    
    echo "Var1: \$var1"
    echo "Var2: \$var2"
    echo "Var3: \$var3"
}

\# Test the complex function
complex_function "test\\"quote" "second_param" "third\\"param"
test_locals "hello" "world"
`,
  '063_01_deeply_nested_arithmetic.sh': `\#!/bin/bash

\# 1. Deeply nested arithmetic expressions with mixed operators
result=\$(( (a + b) * (c - d) / (e % f) + (g ** h) - (i << j) | (k & l) ^ (m | n) ))
echo "Deeply nested arithmetic result: \$result"
`,
  '063_02_complex_array_assignments.sh': `\#!/bin/bash

\# 2. Complex array assignments with nested expansions
declare -A matrix
matrix[0,0]=\$(( (x + y) * z ))
matrix[1,1]=\${array[\${index}]}
matrix[2,2]=\${!prefix@}
matrix[3,3]=\${\#array[@]}

echo "Matrix assignments completed"
`,
  '063_03_nested_command_substitutions.sh': `\#!/bin/bash

\# 3. Nested command substitutions with complex quoting
output=\$(echo "Result: \$(echo "Nested: \$(echo "Deep: \$(echo "Level 4")")")")
echo "\$output"
`,
  '063_04_complex_parameter_expansion.sh': `\#!/bin/bash

\# 4. Complex parameter expansion with nested braces
echo "\${var:-\${default:-\${fallback:-\$(echo "computed")}}}"
echo "\${array[\${index}]:-\${default[@]:0:2}}"
echo "\${!prefix*[@]:0:3}"
`,
  '063_05_heredoc_with_complex_content.sh': `\#!/bin/bash

\# 5. Heredoc with complex content and nested expansions
cat << 'EOF' | grep -v "^\#" | sed 's/^/  /'
\# This is a comment
\$(echo "Command substitution")
\${var:-default}
\$(( 1 + 2 * 3 ))
EOF
`,
  '063_06_complex_pipeline_background.sh': `\#!/bin/bash

\# 6. Complex pipeline with background processes and subshells
(echo "Starting"; sleep 1) &
(echo "Processing"; sleep 2) &
wait
echo "All done"
`,
  '063_07_nested_if_statements.sh': `\#!/bin/bash

\# 7. Nested if statements with complex conditions
if [[ \$var =~ ^[0-9]+\$ ]] && (( var > 0 )) && [ -f "\$file" ]; then
    if [[ \${array[@]} =~ "value" ]] || (( \${\#array[@]} > 5 )); then
        if [ "\$(echo "\$var" | grep -q "pattern")" ]; then
            echo "Deeply nested condition met"
        fi
    fi
fi
`,
  '063_08_complex_case_statement.sh': `\#!/bin/bash

\# 8. Complex case statement with patterns and command substitution
case "\$(echo "\$var" | tr '[:upper:]' '[:lower:]')" in
    *[0-9]*)
        case "\${var,,}" in
            *pattern*)
                echo "Double nested pattern"
                ;;
            *)
                echo "Single nested pattern"
                ;;
        esac
        ;;
    *)
        echo "No numbers"
        ;;
esac
`,
  '063_09_complex_function_parameter_handling.sh': `\#!/bin/bash

\# 9. Function with complex parameter handling and local variables
complex_function() {
    local -a args=("\$@")
    local -A options=()
    local i=0
    
    while (( i < \${\#args[@]} )); do
        case "\${args[i]}" in
            --*)
                local key="\${args[i]\#--}"
                local value="\${args[i+1]:-true}"
                options["\$key"]="\$value"
                (( i += 2 ))
                ;;
            -*)
                local flags="\${args[i]\#-}"
                local j=0
                while (( j < \${\#flags} )); do
                    options["\${flags:j:1}"]="true"
                    (( j++ ))
                done
                (( i++ ))
                ;;
            *)
                break
                ;;
        esac
    done
    
    echo "Processed \${\#options[@]} options"
}

\# Test the function
complex_function --flag1 --option1=value1 -abc
`,
  '063_10_complex_for_loop.sh': ``,
  '063_11_complex_while_loop.sh': `\#!/bin/bash

\# 11. While loop with complex condition and nested commands
while IFS= read -r line && [ -n "\$line" ] && (( counter < max_lines )); do
    if [[ "\$line" =~ ^[[:space:]]*\# ]]; then
        continue
    fi
    
    case "\$line" in
        *\\\$\\(*\\)*)
            echo "Contains command substitution: \$line"
            ;;
        *\\\$\\{[^}]*\\}*)
            echo "Contains parameter expansion: \$line"
            ;;
        *\\\$\\(\\(*\\)\\)*)
            echo "Contains arithmetic expansion: \$line"
            ;;
    esac
    
    (( counter++ ))
done < <(grep -v "^\#" "\$input_file" | head -n "\$max_lines")
`,
  '063_12_complex_eval.sh': `\#!/bin/bash

\# 12. Complex eval with nested expansions
eval "result=\\\$(( \\\${var:-0} + \\\${array[\\\${index:-0}]:-0} ))"
echo "Eval result: \$result"
`,
  '063_13_nested_subshells.sh': `\#!/bin/bash

\# 13. Nested subshells with complex command chains
(
    (
        (
            echo "Level 3"
            (echo "Level 4"; echo "Still level 4")
        ) | grep "Level"
    ) | sed 's/Level/Depth/'
) | wc -l
`,
  '063_14_complex_redirects.sh': `\#!/bin/bash

\# 14. Complex redirects with process substitution
diff <(sort file1.txt) <(sort file2.txt) > comparison.txt 2>&1
`,
  '063_15_complex_function_definition.sh': `\#!/bin/bash

\# 15. Function definition with complex body and nested constructs
define_complex_function() {
    local name="\$1"
    local body="\$2"
    
    eval "\$name() {
        \$body
    }"
}

\# Test the function
define_complex_function "test_func" "echo 'Hello from dynamic function'"
test_func
`,
  '063_16_complex_test_expressions.sh': `\#!/bin/bash

\# 16. Complex test expressions with multiple operators
if [ -n "\$var" -a -f "\$file" -o -d "\$dir" ] && [ "\$(wc -l < "\$file")" -gt 10 ]; then
    echo "Complex test passed"
fi
`,
  '063_17_nested_brace_expansion.sh': `\#!/bin/bash

\# 17. Nested brace expansion with complex patterns
echo {a,b,c}{1..3}{x,y,z}
`,
  '063_18_complex_here_string.sh': `\#!/bin/bash

\# 18. Complex here-string with nested expansions
tr '[:upper:]' '[:lower:]' <<< "\$(echo "UPPER: \${var^^}")"
`,
  '063_19_complex_function_call.sh': `\#!/bin/bash

\# 19. Function call with complex argument processing
complex_function \\
    --long-option="value with spaces" \\
    --array-option=("item1" "item2" "item3") \\
    --flag \\
    "positional argument" \\
    "\${var:-default}" \\
    "\$(echo "computed")"
`,
  '063_20_final_complex_construct.sh': `\#!/bin/bash

\# 20. Final complex construct combining multiple challenging elements
(
    if [[ "\$(echo "\$var" | tr '[:upper:]' '[:lower:]')" =~ ^[a-z]+\$ ]]; then
        for ((i=0; i<\${\#array[@]}; i++)); do
            if (( array[i] > threshold )) && [ -f "\${files[i]}" ]; then
                result[i]=\$(( result[i] + \$(wc -l < "\${files[i]}") ))
            fi
        done
    fi
) | sort -n | tail -n 5 > final_result.txt
`,
  '063_hard_to_parse.sh': `\#!/bin/bash

\# This file contains bash constructs that are particularly challenging to parse
\# due to complex nesting, ambiguous syntax, and edge cases

\# 1. Deeply nested arithmetic expressions with mixed operators
result=\$(( (a + b) * (c - d) / (e % f) + (g ** h) - (i << j) | (k & l) ^ (m | n) ))

\# 2. Complex array assignments with nested expansions
declare -A matrix
matrix[0,0]=\$(( (x + y) * z ))
matrix[1,1]=\${array[\${index}]}
matrix[2,2]=\${!prefix@}
matrix[3,3]=\${\#array[@]}

\# 3. Nested command substitutions with complex quoting
output=\$(echo "Result: \$(echo "Nested: \$(echo "Deep: \$(echo "Level 4")")")")

\# 4. Complex parameter expansion with nested braces
echo "\${var:-\${default:-\${fallback:-\$(echo "computed")}}}"
echo "\${array[\${index}]:-\${default[@]:0:2}}"
echo "\${!prefix*[@]:0:3}"

\# 5. Heredoc with complex content and nested expansions
cat << 'EOF' | grep -v "^\#" | sed 's/^/  /'
\# This is a comment
\$(echo "Command substitution")
\${var:-default}
\$(( 1 + 2 * 3 ))
EOF

\# 6. Complex pipeline with background processes and subshells
(echo "Starting"; sleep 1) &
(echo "Processing"; sleep 2) &
wait
echo "All done"

\# 7. Nested if statements with complex conditions
if [[ \$var =~ ^[0-9]+\$ ]] && (( var > 0 )) && [ -f "\$file" ]; then
    if [[ \${array[@]} =~ "value" ]] || (( \${\#array[@]} > 5 )); then
        if [ "\$(echo "\$var" | grep -q "pattern")" ]; then
            echo "Deeply nested condition met"
        fi
    fi
fi

\# 8. Complex case statement with patterns and command substitution
case "\$(echo "\$var" | tr '[:upper:]' '[:lower:]')" in
    *[0-9]*)
        case "\${var,,}" in
            *pattern*)
                echo "Double nested pattern"
                ;;
            *)
                echo "Single nested pattern"
                ;;
        esac
        ;;
    *)
        echo "No numbers"
        ;;
esac

\# 9. Function with complex parameter handling and local variables
complex_function() {
    local -a args=("\$@")
    local -A options=()
    local i=0
    
    while (( i < \${\#args[@]} )); do
        case "\${args[i]}" in
            --*)
                local key="\${args[i]\#--}"
                local value="\${args[i+1]:-true}"
                options["\$key"]="\$value"
                (( i += 2 ))
                ;;
            -*)
                local flags="\${args[i]\#-}"
                local j=0
                while (( j < \${\#flags} )); do
                    options["\${flags:j:1}"]="true"
                    (( j++ ))
                done
                (( i++ ))
                ;;
            *)
                break
                ;;
        esac
    done
    
    echo "Processed \${\#options[@]} options"
}

\# 10. Complex for loop with arithmetic and array manipulation
for ((i=0; i<\${\#array[@]}; i++)); do
    for ((j=0; j<\${\#array[i][@]}; j++)); do
        if (( array[i][j] > threshold )); then
            result[i]=\$(( result[i] + array[i][j] ))
        fi
    done
done

\# 11. While loop with complex condition and nested commands
while IFS= read -r line && [ -n "\$line" ] && (( counter < max_lines )); do
    if [[ "\$line" =~ ^[[:space:]]*\# ]]; then
        continue
    fi
    
    case "\$line" in
        *\\\$\\(*\\)*)
            echo "Contains command substitution: \$line"
            ;;
        *\\\$\\{[^}]*\\}*)
            echo "Contains parameter expansion: \$line"
            ;;
        *\\\$\\(\\(*\\)\\)*)
            echo "Contains arithmetic expansion: \$line"
            ;;
    esac
    
    (( counter++ ))
done < <(grep -v "^\#" "\$input_file" | head -n "\$max_lines")

\# 12. Complex eval with nested expansions
eval "result=\\\$(( \\\${var:-0} + \\\${array[\\\${index:-0}]:-0} ))"

\# 13. Nested subshells with complex command chains
(
    (
        (
            echo "Level 3"
            (echo "Level 4"; echo "Still level 4")
        ) | grep "Level"
    ) | sed 's/Level/Depth/'
) | wc -l

\# 14. Complex redirects with process substitution
diff <(sort file1.txt) <(sort file2.txt) > comparison.txt 2>&1

\# 15. Function definition with complex body and nested constructs
define_complex_function() {
    local name="\$1"
    local body="\$2"
    
    eval "\$name() {
        \$body
    }"
}

\# 16. Complex test expressions with multiple operators
if [ -n "\$var" -a -f "\$file" -o -d "\$dir" ] && [ "\$(wc -l < "\$file")" -gt 10 ]; then
    echo "Complex test passed"
fi

\# 17. Nested brace expansion with complex patterns
echo {a,b,c}{1..3}{x,y,z}

\# 18. Complex here-string with nested expansions
tr '[:upper:]' '[:lower:]' <<< "\$(echo "UPPER: \${var^^}")"

\# 19. Function call with complex argument processing
complex_function \\
    --long-option="value with spaces" \\
    --array-option=("item1" "item2" "item3") \\
    --flag \\
    "positional argument" \\
    "\${var:-default}" \\
    "\$(echo "computed")"

\# 20. Final complex construct combining multiple challenging elements
(
    if [[ "\$(echo "\$var" | tr '[:upper:]' '[:lower:]')" =~ ^[a-z]+\$ ]]; then
        for ((i=0; i<\${\#array[@]}; i++)); do
            if (( array[i] > threshold )) && [ -f "\${files[i]}" ]; then
                result[i]=\$(( result[i] + \$(wc -l < "\${files[i]}") ))
            fi
        done
    fi
) | sort -n | tail -n 5 > final_result.txt
`,
  '064_01_complex_nested_subshells.sh': `\#!/bin/bash

\# 1. Complex nested subshells with process substitution
diff <(sort <(grep -v "^\#" /etc/passwd | cut -d: -f1)) <(sort <(grep -v "^\#" /etc/group | cut -d: -f1))
`,
  '064_02_nested_brace_expansions.sh': `\#!/bin/bash

\# 2. Nested brace expansions with ranges and sequences
echo "Files: " file_{a..z}_{1..10,20,30..40}.{txt,log,dat}
`,
  '064_03_complex_parameter_expansion.sh': `\#!/bin/bash

\# 3. Complex parameter expansion with nested substitutions
name="John Doe"
echo "Hello \${name// /_}"  \# Replace spaces with underscores
echo "Length: \${\#name}"    \# String length
echo "First: \${name:0:4}"  \# Substring
echo "Last: \${name: -3}"   \# Last 3 characters
`,
  '064_04_extended_glob_patterns.sh': `\#!/bin/bash

\# 4. Extended glob patterns with shopt
shopt -s extglob
shopt -s nocasematch

echo "Extended glob patterns enabled"
`,
  '064_05_complex_case_statement.sh': `\#!/bin/bash

\# 5. Complex case statement with pattern matching
case "\$1" in
    [a-z]*) echo "Lowercase start";;
    [A-Z]*) echo "Uppercase start";;
    [0-9]*) echo "Number start";;
    ?) echo "Single character";;
    *) echo "Something else";;
esac
`,
  '064_06_nested_arithmetic_expressions.sh': `\#!/bin/bash

\# 6. Nested arithmetic expressions
((i = 1 + (2 * 3) / 4))
((j = i++ + ++i))
echo "i=\$i, j=\$j"
`,
  '064_07_complex_array_operations.sh': `\#!/bin/bash

\# 7. Complex array operations with associative arrays
declare -A config
config["user"]="admin"
config["host"]="localhost"
config["port"]="8080"

echo "Config: \${config[@]}"
`,
  '064_08_heredocs_with_variable_interpolation.sh': `\#!/bin/bash

\# 8. Here-documents with variable interpolation
cat <<'EOF' > config.txt
User: \$USER
Host: \${HOSTNAME:-localhost}
Path: \$PWD
EOF

echo "Config file created"
`,
  '064_09_process_substitution_pipeline.sh': `\#!/bin/bash

\# 9. Process substitution in pipeline with complex commands
paste <(cut -d: -f1 /etc/passwd | sort) <(cut -d: -f3 /etc/passwd | sort -n) | head -10
`,
  '064_10_nested_function_definitions.sh': `\#!/bin/bash

\# 10. Nested function definitions with local variables
outer_func() {
    local outer_var="outer"
    
    inner_func() {
        local inner_var="inner"
        echo "Outer: \$outer_var, Inner: \$inner_var"
        
        \# Nested arithmetic
        ((result = outer_var + inner_var))
        echo "Result: \$result"
    }
    
    inner_func
}

\# Test the nested functions
outer_func
`,
  '064_11_complex_test_expressions.sh': `\#!/bin/bash

\# 11. Complex test expressions with extended operators
if [[ "\$1" =~ ^[0-9]+\$ ]] && [[ "\$2" == "test" || "\$2" == "debug" ]]; then
    echo "Valid input"
fi
`,
  '064_12_brace_expansion_nested_sequences.sh': `\#!/bin/bash

\# 12. Brace expansion with nested sequences
mkdir -p project/{src/{main,test}/{java,resources},docs/{api,user},build/{classes,lib}}
echo "Project structure created"
`,
  '064_13_complex_string_manipulation.sh': `\#!/bin/bash

\# 13. Complex string manipulation with parameter expansion
filename="my_file.txt"
basename="\${filename%.*}"           \# Remove extension
extension="\${filename\#\#*.}"         \# Get extension
uppercase="\${filename^^}"           \# Convert to uppercase
lowercase="\${filename,,}"           \# Convert to lowercase

echo "Basename: \$basename"
echo "Extension: \$extension"
echo "Uppercase: \$uppercase"
echo "Lowercase: \$lowercase"
`,
  '064_14_nested_command_substitution_arithmetic.sh': `\#!/bin/bash

\# 14. Nested command substitution with arithmetic
result=\$(echo \$(( \$(wc -l < /etc/passwd) + \$(wc -l < /etc/group) )))
echo "Total lines: \$result"
`,
  '064_15_complex_pipeline_multiple_redirects.sh': `\#!/bin/bash

\# 15. Complex pipeline with multiple redirects
grep -v "^\#" /etc/passwd | cut -d: -f1,3 | sort -t: -k2 -n | head -5 > users.txt 2> errors.log
echo "Pipeline completed"
`,
  '064_16_function_complex_argument_handling.sh': `\#!/bin/bash

\# 16. Function with complex argument handling
process_files() {
    local -a files=("\$@")
    local count=0
    
    for file in "\${files[@]}"; do
        if [[ -f "\$file" ]]; then
            ((count++))
            echo "Processing: \$file"
        fi
    done
    
    echo "Total files processed: \$count"
}

\# Test the function
process_files "file1.txt" "file2.txt" "nonexistent.txt"
`,
  '064_17_complex_while_loop_nested_conditionals.sh': `\#!/bin/bash

\# 17. Complex while loop with nested conditionals
while IFS=: read -r user pass uid gid info home shell; do
    if [[ "\$uid" -gt 1000 ]] && [[ "\$shell" != "/bin/false" ]]; then
        if [[ "\$home" =~ ^/home/ ]]; then
            echo "User: \$user (UID: \$uid) - \$home"
        fi
    fi
done < /etc/passwd
`,
  '064_18_array_slicing_manipulation.sh': `\#!/bin/bash

\# 18. Array slicing and manipulation
numbers=(1 2 3 4 5 6 7 8 9 10)
middle=("\${numbers[@]:3:4}")        \# Elements 4-7
first_half=("\${numbers[@]:0:5}")   \# First 5 elements
last_half=("\${numbers[@]:5}")      \# Last 5 elements

echo "Middle: \${middle[@]}"
echo "First half: \${first_half[@]}"
echo "Last half: \${last_half[@]}"
`,
  '064_19_complex_pattern_matching_extended_globs.sh': `\#!/bin/bash

\# 19. Complex pattern matching with extended globs
for file in *.{txt,log,dat}; do
    case "\$file" in
        @(*.txt|*.log)) echo "Text file: \$file";;
        *.dat) echo "Data file: \$file";;
        *) echo "Other file: \$file";;
    esac
done
`,
  '064_20_nested_subshells_environment_variables.sh': `\#!/bin/bash

\# 20. Nested subshells with environment variables
(
    export DEBUG=1
    export LOG_LEVEL=verbose
    
    (
        unset DEBUG
        echo "Inner: LOG_LEVEL=\$LOG_LEVEL, DEBUG=\${DEBUG:-unset}"
    )
    
    echo "Outer: LOG_LEVEL=\$LOG_LEVEL, DEBUG=\$DEBUG"
)
`,
  '064_21_complex_string_interpolation_multiple_variables.sh': `\#!/bin/bash

\# 21. Complex string interpolation with multiple variables
message="Hello \${USER:-guest} from \${HOSTNAME:-localhost}"
echo "\$message"
`,
  '064_22_function_returning_complex_data_structures.sh': `\#!/bin/bash

\# 22. Function returning complex data structures
get_system_info() {
    local -A info
    info["os"]="\$(uname -s)"
    info["arch"]="\$(uname -m)"
    info["hostname"]="\$(hostname)"
    info["user"]="\$USER"
    
    \# Return as associative array (bash 4+)
    declare -p info
}

\# Test the function
get_system_info
`,
  '064_23_complex_error_handling_traps.sh': `\#!/bin/bash

\# 23. Complex error handling with traps
trap 'echo "Error on line \$LINENO"; exit 1' ERR
trap 'echo "Cleaning up..."; rm -f /tmp/temp_*' EXIT

echo "Traps set up"
`,
  '064_24_advanced_parameter_expansion.sh': `\#!/bin/bash

\# 24. Advanced parameter expansion with default values and transformations
input="\${1:-default_value}"
sanitized="\${input//[^a-zA-Z0-9]/_}"
uppercase="\${sanitized^^}"
echo "Input: '\$input' -> Sanitized: '\$sanitized' -> Uppercase: '\$uppercase'"
`,
  '064_25_complex_command_chaining.sh': ``,
  '064_hard_to_generate.sh': `\#!/bin/bash

\# This script combines multiple complex bash features that are challenging to parse and generate
\# It tests the limits of the bash-to-perl converter

\# 1. Complex nested subshells with process substitution
diff <(sort <(grep -v "^\#" /etc/passwd | cut -d: -f1)) <(sort <(grep -v "^\#" /etc/group | cut -d: -f1))

\# 2. Nested brace expansions with ranges and sequences
echo "Files: " file_{a..z}_{1..10,20,30..40}.{txt,log,dat}

\# 3. Complex parameter expansion with nested substitutions
name="John Doe"
echo "Hello \${name// /_}"  \# Replace spaces with underscores
echo "Length: \${\#name}"    \# String length
echo "First: \${name:0:4}"  \# Substring
echo "Last: \${name: -3}"   \# Last 3 characters

\# 4. Extended glob patterns with shopt
shopt -s extglob
shopt -s nocasematch

\# 5. Complex case statement with pattern matching
case "\$1" in
    [a-z]*) echo "Lowercase start";;
    [A-Z]*) echo "Uppercase start";;
    [0-9]*) echo "Number start";;
    ?) echo "Single character";;
    *) echo "Something else";;
esac

\# 6. Nested arithmetic expressions
((i = 1 + (2 * 3) / 4))
((j = i++ + ++i))
echo "i=\$i, j=\$j"

\# 7. Complex array operations with associative arrays
declare -A config
config["user"]="admin"
config["host"]="localhost"
config["port"]="8080"

\# 8. Here-documents with variable interpolation
cat <<'EOF' > config.txt
User: \$USER
Host: \${HOSTNAME:-localhost}
Path: \$PWD
EOF

\# 9. Process substitution in pipeline with complex commands
paste <(cut -d: -f1 /etc/passwd | sort) <(cut -d: -f3 /etc/passwd | sort -n) | head -10

\# 10. Nested function definitions with local variables
outer_func() {
    local outer_var="outer"
    
    inner_func() {
        local inner_var="inner"
        echo "Outer: \$outer_var, Inner: \$inner_var"
        
        \# Nested arithmetic
        ((result = outer_var + inner_var))
        echo "Result: \$result"
    }
    
    inner_func
}

\# 11. Complex test expressions with extended operators
if [[ "\$1" =~ ^[0-9]+\$ ]] && [[ "\$2" == "test" || "\$2" == "debug" ]]; then
    echo "Valid input"
fi

\# 12. Brace expansion with nested sequences
mkdir -p project/{src/{main,test}/{java,resources},docs/{api,user},build/{classes,lib}}

\# 13. Complex string manipulation with parameter expansion
filename="my_file.txt"
basename="\${filename%.*}"           \# Remove extension
extension="\${filename\#\#*.}"         \# Get extension
uppercase="\${filename^^}"           \# Convert to uppercase
lowercase="\${filename,,}"           \# Convert to lowercase

\# 14. Nested command substitution with arithmetic
result=\$(echo \$(( \$(wc -l < /etc/passwd) + \$(wc -l < /etc/group) )))

\# 15. Complex pipeline with multiple redirects
grep -v "^\#" /etc/passwd | cut -d: -f1,3 | sort -t: -k2 -n | head -5 > users.txt 2> errors.log

\# 16. Function with complex argument handling
process_files() {
    local -a files=("\$@")
    local count=0
    
    for file in "\${files[@]}"; do
        if [[ -f "\$file" ]]; then
            ((count++))
            echo "Processing: \$file"
        fi
    done
    
    echo "Total files processed: \$count"
}

\# 17. Complex while loop with nested conditionals
while IFS=: read -r user pass uid gid info home shell; do
    if [[ "\$uid" -gt 1000 ]] && [[ "\$shell" != "/bin/false" ]]; then
        if [[ "\$home" =~ ^/home/ ]]; then
            echo "User: \$user (UID: \$uid) - \$home"
        fi
    fi
done < /etc/passwd

\# 18. Array slicing and manipulation
numbers=(1 2 3 4 5 6 7 8 9 10)
middle=("\${numbers[@]:3:4}")        \# Elements 4-7
first_half=("\${numbers[@]:0:5}")   \# First 5 elements
last_half=("\${numbers[@]:5}")      \# Last 5 elements

\# 19. Complex pattern matching with extended globs
for file in *.{txt,log,dat}; do
    case "\$file" in
        @(*.txt|*.log)) echo "Text file: \$file";;
        *.dat) echo "Data file: \$file";;
        *) echo "Other file: \$file";;
    esac
done

\# 20. Nested subshells with environment variables
(
    export DEBUG=1
    export LOG_LEVEL=verbose
    
    (
        unset DEBUG
        echo "Inner: LOG_LEVEL=\$LOG_LEVEL, DEBUG=\${DEBUG:-unset}"
    )
    
    echo "Outer: LOG_LEVEL=\$LOG_LEVEL, DEBUG=\$DEBUG"
)

\# 21. Complex string interpolation with multiple variables
message="Hello \${USER:-guest} from \${HOSTNAME:-localhost}"
echo "\$message"

\# 22. Function returning complex data structures
get_system_info() {
    local -A info
    info["os"]="\$(uname -s)"
    info["arch"]="\$(uname -m)"
    info["hostname"]="\$(hostname)"
    info["user"]="\$USER"
    
    \# Return as associative array (bash 4+)
    declare -p info
}

\# 23. Complex error handling with traps
trap 'echo "Error on line \$LINENO"; exit 1' ERR
trap 'echo "Cleaning up..."; rm -f /tmp/temp_*' EXIT

\# 24. Advanced parameter expansion with default values and transformations
input="\${1:-default_value}"
sanitized="\${input//[^a-zA-Z0-9]/_}"
uppercase="\${sanitized^^}"
echo "Input: '\$input' -> Sanitized: '\$sanitized' -> Uppercase: '\$uppercase'"

\# 25. Complex command chaining with logical operators
[[ -f "\$1" ]] && echo "File exists" || echo "File not found"
[[ -d "\$2" ]] && cd "\$2" && pwd || echo "Directory not accessible"

echo "Script completed successfully!"
`,
  '065_yes_head_while.sh': `yes Line:LINE | head -n100 | while read L; do i=\$((i+1)); echo \$L | sed s/LINE/\$i/ ; done

\#Avoid arrays, use a line by line pipeline rather than buffered.
\#PERL_MUST_NOT_CONTAIN: @

\#Only use basename and main_exit_code if actually needed.
\#PERL_MUST_NOT_CONTAIN: Basename
\#PERL_MUST_NOT_CONTAIN: main_exit_code

\#Not sure why this would appear, but it did
\#PERL_MUST_NOT_CONTAIN: \$lines=\$L
`,
  '900_if2echo.sh': `  if [ \$\# -lt 2 ]; then
    echo "One"
    echo "Two"
  fi

\#Cleaner to have word and newline in same quotes
\#PERL_MUST_CONTAIN "One\\n"
`,
  '999_pwd.sh': `basename \`pwd\`
`,
  'comparison.txt': ``,
  'config.txt': `User: \$USER
Host: \${HOSTNAME:-localhost}
Path: \$PWD
`,
  'stderr.txt': ``,
  'test_binary.txt': `Hello World
This is a test
Binary content here
`,
  'test_find.sh': `\#!/bin/bash
find . -name "*.sh" | head -3
`,
  'test_grep.sh': `result=\`grep pattern file.txt\`
`,
  'test_perl_critic.sh': `echo "Testing Perl::Critic integration"
`,
  'test_simple_function.sh': `\#!/bin/bash

get_file_size() {
    local file=\$1
    local size=\`wc -c < "\$file"\`
    echo "File \$file has \$size bytes"
}

get_file_size test_simple_function.sh
`,
  'test_system_builtin.sh': `\#!/bin/bash

\# This script should generate system calls with builtin commands
echo "Testing system calls with builtin commands"

\# These should generate system 'ls' and system 'find' calls
result1=\`ls -la\`
result2=\`find . -name "*.txt"\`

echo "Results:"
echo "\$result1"
echo "\$result2"
`
};

// Helper function to get all example names
export function getExampleNames() {
  return Object.keys(examples);
}

// Helper function to get example by name
export function getExample(name) {
  return examples[name] || null;
}

// Helper function to get examples grouped by category
export function getExamplesByCategory() {
  const categories = {
    'ANSI Quoting': ['014_ansi_quoting.sh', '020_ansi_quoting_basic.sh', '021_ansi_quoting_escape.sh', '022_ansi_quoting_unicode.sh', '023_ansi_quoting_practical.sh'],
    'Advanced Examples': ['000__04h_complex_examples.sh', '062_03_complex_heredocs.sh', '062_05_nested_command_substitution.sh', '062_12_complex_string_interpolation.sh', '062_hard_to_lex.sh', '063_03_nested_command_substitutions.sh', '063_05_heredoc_with_complex_content.sh', '063_12_complex_eval.sh', '063_13_nested_subshells.sh', '063_14_complex_redirects.sh', '063_16_complex_test_expressions.sh', '063_18_complex_here_string.sh', '063_20_final_complex_construct.sh', '063_hard_to_parse.sh', '064_01_complex_nested_subshells.sh', '064_11_complex_test_expressions.sh', '064_13_complex_string_manipulation.sh', '064_20_nested_subshells_environment_variables.sh', '064_21_complex_string_interpolation_multiple_variables.sh', '064_23_complex_error_handling_traps.sh', '064_25_complex_command_chaining.sh', '064_hard_to_generate.sh'],
    'Arithmetic & Math': ['051_primes.sh', '052_numeric_computations.sh', '053_gcd.sh', '054_fibonacci.sh', '055_factorize.sh', '062_04_nested_arithmetic.sh', '062_11_mixed_arithmetic.sh', '063_01_deeply_nested_arithmetic.sh', '064_06_nested_arithmetic_expressions.sh', '064_14_nested_command_substitution_arithmetic.sh'],
    'Arrays': ['009_arrays.sh', '028_arrays_indexed.sh', '029_arrays_associative.sh', '062_14_complex_array_operations.sh', '063_02_complex_array_assignments.sh', '064_07_complex_array_operations.sh', '064_18_array_slicing_manipulation.sh'],
    'Basic Examples': ['000__01_file_directory_operations.sh', '000__02_output_formatting_commands.sh', '000__03_file_manipulation_commands.sh', '000__04a_basic_command_substitution.sh', '000__04b_file_directory_operations.sh', '000__04c_text_processing_commands.sh', '000__04d_system_utilities.sh', '000__04e_file_manipulation.sh', '000__04f_output_formatting.sh', '000__04g_checksum_commands.sh', '000__05_system_utilities.sh', '000__06_text_processing_commands.sh', '001_simple.sh', '004_test_quoted.sh', '005_args.sh', '006_misc.sh', '007_cat_EOF.sh', '008_simple_backup.sh', '045_shell_calling_perl.sh', '047_for_arithematic.sh', '050_test_ls_star_dot_sh.sh', '056_send_args.sh', '058_advanced_bash_idioms.sh', '062_01_ambiguous_operators.sh', '062_13_simple_test_expressions.sh', '064_04_extended_glob_patterns.sh', '064_08_heredocs_with_variable_interpolation.sh', '065_yes_head_while.sh', '999_pwd.sh', 'test_perl_critic.sh', 'test_system_builtin.sh'],
    'Brace Expansion': ['011_brace_expansion.sh', '033_brace_expansion_basic.sh', '034_brace_expansion_advanced.sh', '035_brace_expansion_practical.sh', '062_07_complex_brace_expansion.sh', '063_17_nested_brace_expansion.sh', '064_02_nested_brace_expansions.sh', '064_12_brace_expansion_nested_sequences.sh'],
    'Control Flow': ['002_control_flow.sh', '024_parameter_expansion_case.sh', '030_control_flow_if.sh', '031_control_flow_loops.sh', '032_control_flow_function.sh', '038_pattern_matching_nocase.sh', '057_case.sh', '062_08_simple_case.sh', '062_09_complex_function.sh', '063_07_nested_if_statements.sh', '063_08_complex_case_statement.sh', '063_09_complex_function_parameter_handling.sh', '063_10_complex_for_loop.sh', '063_11_complex_while_loop.sh', '063_15_complex_function_definition.sh', '063_19_complex_function_call.sh', '064_05_complex_case_statement.sh', '064_10_nested_function_definitions.sh', '064_16_function_complex_argument_handling.sh', '064_17_complex_while_loop_nested_conditionals.sh', '064_22_function_returning_complex_data_structures.sh', '900_if2echo.sh', 'test_simple_function.sh'],
    'Data Files': ['comparison.txt', 'config.txt', 'stderr.txt', 'test_binary.txt'],
    'File Operations': ['000__07_find_path_commands.sh', '043_home.sh', '044_find_example.sh', 'test_find.sh'],
    'Grep Examples': ['015_grep_advanced.sh', '016_grep_basic.sh', '017_grep_context.sh', '018_grep_params.sh', '019_grep_regex.sh', 'test_grep.sh'],
    'Issue Examples': ['059_issue3.sh', '060_issue5.sh'],
    'Parameter Expansion': ['013_parameter_expansion.sh', '025_parameter_expansion_advanced.sh', '026_parameter_expansion_more.sh', '027_parameter_expansion_defaults.sh', '062_02_complex_parameter_expansions.sh', '063_04_complex_parameter_expansion.sh', '064_03_complex_parameter_expansion.sh', '064_24_advanced_parameter_expansion.sh'],
    'Pattern Matching': ['010_pattern_matching.sh', '036_pattern_matching_basic.sh', '037_pattern_matching_extglob.sh', '064_19_complex_pattern_matching_extended_globs.sh'],
    'Pipelines': ['003_pipeline.sh', '062_10_simple_pipeline.sh', '063_06_complex_pipeline_background.sh', '064_09_process_substitution_pipeline.sh', '064_15_complex_pipeline_multiple_redirects.sh'],
    'Process Substitution': ['012_process_substitution.sh', '039_process_substitution_here.sh', '040_process_substitution_comm.sh', '041_process_substitution_mapfile.sh', '042_process_substitution_advanced.sh', '062_06_process_substitution.sh'],
    'Shell Operations': ['046_cd..sh', '048_subprocess.sh', '049_local.sh', '061_test_local_names_preserved.sh', '062_15_complex_local_variables.sh']
  };
  
  return categories;
}

// Helper function to get examples as JSON (for compatibility with existing code)
export function examplesJson() {
  return JSON.stringify(Object.entries(examples).map(([name, content]) => ({
    name,
    content
  })));
}

#!/bin/bash

# 11. While loop with complex condition and nested commands
input_file=$(mktemp)
max_lines=3
counter=0
printf '# comment\nhas $(sub) here\nplain line\n${param} line\n' > "$input_file"
while IFS= read -r line && [ -n "$line" ] && (( counter < max_lines )); do
    if [[ "$line" =~ ^[[:space:]]*# ]]; then
        continue
    fi
    
    case "$line" in
        *\$\(*\)*)
            echo "Contains command substitution: $line"
            ;;
        *\$\{[^}]*\}*)
            echo "Contains parameter expansion: $line"
            ;;
        *\$\(\(*\)\)*)
            echo "Contains arithmetic expansion: $line"
            ;;
    esac
    
    (( counter++ ))
done < <(grep -v "^#" "$input_file" | head -n "$max_lines")
rm -f "$input_file"

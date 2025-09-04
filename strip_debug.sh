#!/bin/bash

# Script to strip out all DEBUG output from Rust source files
# This removes eprintln! statements that contain "DEBUG:" and related debug output

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Stripping DEBUG output from Rust files${NC}"
echo -e "${BLUE}========================================${NC}"

# Find all Rust files in src directory
rust_files=($(find src -name "*.rs" -type f))

total_files=0
total_lines=0

# Process each Rust file
for file in "${rust_files[@]}"; do
    if [[ -f "$file" ]]; then
        echo -e "${YELLOW}Processing: $file${NC}"
        
        # Create a temporary file
        temp_file=$(mktemp 2>/dev/null || echo "${file}.tmp")
        
        # Process the file line by line
        line_count=0
        while IFS= read -r line; do
            # Check if line contains DEBUG: and is an eprintln! statement
            if [[ "$line" =~ eprintln!.*DEBUG: ]] && [[ ! "$line" =~ ^[[:space:]]*// ]]; then
                # Comment out the line by adding // at the beginning
                echo "// $line" >> "$temp_file"
                ((line_count++))
                echo -e "  ${GREEN}Commented: $line${NC}"
            else
                # Keep the line as is
                echo "$line" >> "$temp_file"
            fi
        done < "$file"
        
        # Replace original file with temporary file
        if [[ $line_count -gt 0 ]]; then
            if mv "$temp_file" "$file" 2>/dev/null; then
                echo -e "  ${GREEN}Modified $file: $line_count DEBUG lines commented${NC}"
                ((total_lines += line_count))
            else
                echo -e "  ${RED}Error: Failed to update $file${NC}"
                rm -f "$temp_file"
            fi
        else
            rm -f "$temp_file"
            echo -e "  ${BLUE}No changes needed in $file${NC}"
        fi
        
        ((total_files++))
    fi
done

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Processing complete!${NC}"
echo -e "${GREEN}Files processed: $total_files${NC}"
echo -e "${GREEN}Total DEBUG lines commented: $total_lines${NC}"
echo -e "${GREEN}========================================${NC}"

# Optional: Show a summary of what was changed
if [[ $total_lines -gt 0 ]]; then
    echo -e "${YELLOW}Summary of changes:${NC}"
    echo -e "${YELLOW}All eprintln! statements containing 'DEBUG:' have been commented out.${NC}"
    echo -e "${YELLOW}You can now use --debug and --no-debug flags to control debug output.${NC}"
    echo -e "${YELLOW}To restore debug output, uncomment the lines or use the restore_debug.sh script.${NC}"
fi

echo -e "${BLUE}Done!${NC}"

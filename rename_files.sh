#!/bin/bash

# Script to rename files from 06[234]_[0-9]{2}_.... to 062_... and 124_... patterns
# This script handles the renaming of test files in the examples directory

set -e  # Exit on any error

# Function to show usage
show_usage() {
    echo "Usage: $0 [options]"
    echo "Options:"
    echo "  -d, --directory DIR    Directory to process (default: examples)"
    echo "  -n, --dry-run          Show what would be renamed without actually renaming"
    echo "  -v, --verbose          Show detailed output"
    echo "  -h, --help             Show this help message"
    echo ""
    echo "This script renames files matching the pattern:"
    echo "  06[234]_[0-9]{2}_.... -> 062_..."
    echo "  (and potentially other patterns to 124_...)"
}

# Default values
TARGET_DIR="examples"
DRY_RUN=false
VERBOSE=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -d|--directory)
            TARGET_DIR="$2"
            shift 2
            ;;
        -n|--dry-run)
            DRY_RUN=true
            shift
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Check if target directory exists
if [[ ! -d "$TARGET_DIR" ]]; then
    echo "Error: Directory '$TARGET_DIR' does not exist"
    exit 1
fi

echo "Processing directory: $TARGET_DIR"
if [[ "$DRY_RUN" == true ]]; then
    echo "DRY RUN MODE - No files will be actually renamed"
fi
echo ""

# Counter for statistics
total_files=0
renamed_files=0
skipped_files=0

# Process files matching 06[234]_[0-9]{2}_.... pattern
echo "Looking for files matching pattern: 06[234]_[0-9]{2}_...."
echo ""

# Find all files matching the pattern and sort them
find "$TARGET_DIR" -maxdepth 1 -type f -name "06[234]_[0-9][0-9]_*.sh" | sort | while read -r file; do
    total_files=$((total_files + 1))
    
    # Extract the filename without path
    filename=$(basename "$file")
    
    # Extract the parts: 06[234], [0-9][0-9], and the rest
    if [[ $filename =~ ^(06[234])_([0-9][0-9])_(.+)$ ]]; then
        prefix="${BASH_REMATCH[1]}"
        number="${BASH_REMATCH[2]}"
        suffix="${BASH_REMATCH[3]}"
        
        # Create new filename: 062_<number>_<suffix>
        new_filename="062_${number}_${suffix}"
        
        # Full path for new filename
        new_filepath="$TARGET_DIR/$new_filename"
        
        if [[ "$VERBOSE" == true ]]; then
            echo "File: $filename"
            echo "  Prefix: $prefix"
            echo "  Number: $number"
            echo "  Suffix: $suffix"
            echo "  New name: $new_filename"
        fi
        
        # Check if target file already exists
        if [[ -f "$new_filepath" ]]; then
            echo "WARNING: Target file '$new_filename' already exists, skipping '$filename'"
            skipped_files=$((skipped_files + 1))
            continue
        fi
        
        if [[ "$DRY_RUN" == true ]]; then
            echo "Would rename: $filename -> $new_filename"
        else
            if mv "$file" "$new_filepath"; then
                echo "Renamed: $filename -> $new_filename"
                renamed_files=$((renamed_files + 1))
            else
                echo "ERROR: Failed to rename $filename"
                exit 1
            fi
        fi
    else
        echo "WARNING: File '$filename' doesn't match expected pattern, skipping"
        skipped_files=$((skipped_files + 1))
    fi
    
    echo ""
done

# Also look for files that might need to be renamed to 124_... pattern
# This is a placeholder - you may need to adjust the pattern based on your needs
echo "Looking for files that might need 124_... pattern renaming..."
echo ""

# Example: if you want to rename files starting with certain patterns to 124_...
# Uncomment and modify the following section as needed:
#
# find "$TARGET_DIR" -maxdepth 1 -type f -name "099_*.sh" | sort | while read -r file; do
#     filename=$(basename "$file")
#     if [[ $filename =~ ^099_([0-9][0-9])_(.+)$ ]]; then
#         number="${BASH_REMATCH[1]}"
#         suffix="${BASH_REMATCH[2]}"
#         new_filename="124_${number}_${suffix}"
#         new_filepath="$TARGET_DIR/$new_filename"
#         
#         if [[ -f "$new_filepath" ]]; then
#             echo "WARNING: Target file '$new_filename' already exists, skipping '$filename'"
#             continue
#         fi
#         
#         if [[ "$DRY_RUN" == true ]]; then
#             echo "Would rename: $filename -> $new_filename"
#         else
#             if mv "$file" "$new_filepath"; then
#                 echo "Renamed: $filename -> $new_filename"
#                 renamed_files=$((renamed_files + 1))
#             else
#                 echo "ERROR: Failed to rename $filename"
#                 exit 1
#             fi
#         fi
#     fi
# done

echo ""
echo "Summary:"
echo "  Total files processed: $total_files"
echo "  Files renamed: $renamed_files"
echo "  Files skipped: $skipped_files"

if [[ "$DRY_RUN" == true ]]; then
    echo ""
    echo "This was a dry run. To actually rename files, run without --dry-run option."
fi

echo ""
echo "Done!"


#!/usr/bin/env python3
"""Fix UTF-8 char boundary issues in parse_string_interpolation.
Reads the file, makes targeted replacements, writes back.
Uses binary-safe reading to avoid escaping issues.
"""

with open('src/parser/words.rs', 'rb') as f:
    data = f.read()

# We need to find and fix all occurrences of the pattern:
# while i < content.len() && ... {
#     i += 1;
# }
# inside the parse_string_interpolation function
# where the while body should advance by character width.

# The fix is to replace:
#     i += 1;
# with:
#     let ch = content[i..].chars().next().unwrap_or('?');
#     i += ch.len_utf8();
# in the context of scanning while loops.

# Strategy: find the function, then find all while loops with i += 1; body

# The file content as a string (for searching)
text = data.decode('utf-8')

# Find the function boundaries
fn_start_marker = b'fn parse_string_interpolation(lexer: &mut Lexer) -> Result<Word, ParserError> {'
fn_start = data.find(fn_start_marker)
if fn_start < 0:
    print("ERROR: Could not find function start")
    exit(1)

# Find the next function definition (or end of file)
fn_end = data.find(b'\nfn ', fn_start + 100)
if fn_end < 0:
    fn_end = len(data)

func_text = data[fn_start:fn_end]
print(f"Function found at bytes {fn_start}-{fn_end} ({fn_end - fn_start} bytes)")

# Now we need to replace all 'i += 1;' inside scanning while loops
# We'll process the function byte range

# Strategy: 
# 1. Find each 'while ... {' block
# 2. Check if it contains 'i += 1;'
# 3. If yes, check if the 'i += 1;' is the only statement in the body (single-statement while)
# 4. Replace accordingly

import re

# Convert func_text back to string for regex operations
func_str = func_text.decode('utf-8')

# Pattern for single-statement while loops with i += 1;
# Match 'while ... {' followed by whitespace and 'i += 1;' followed by '}'
pattern = re.compile(
    r'(while\s+i\s*<\s*content\.len\(\)\s*&&\s*[^{]+\{)\s*\n(\s*)i\s*\+=\s*1;\s*\n(\s*)\}',
    re.MULTILINE
)

def replace_while(m):
    """Replace i += 1; with proper char-width advancing in while loop."""
    header = m.group(1)
    indent = m.group(2)
    closing_indent = m.group(3)
    
    new_body = f'{indent}let ch = content[i..].chars().next().unwrap_or(\'?\');\n{indent}i += ch.len_utf8();\n{closing_indent}}}'
    return header + '\n' + new_body

new_func_str, count = pattern.subn(replace_while, func_str)
if count == 0:
    # Try alternative pattern - maybe the while body has a different structure
    print("No single-statement while loops found with i += 1;")
    print("Searching for alternative patterns...")
    
    # Find all 'i += 1;' that appear after a 'while' within the function
    lines = func_str.split('\n')
    for idx, line in enumerate(lines):
        stripped = line.strip()
        if stripped == 'i += 1;':
            # Look backward for 'while' keyword
            for j in range(max(0, idx - 10), idx):
                if 'while' in lines[j]:
                    print(f"  Line {fn_start + idx + 1}: {stripped} (after while at line {j+1})")
                    break
else:
    print(f"Replaced {count} while loop(s)")

# Now fix the else branches
# The pattern is:
#     } else {
#         // Add to current literal
#         current_literal.push(content[i..].chars().next().unwrap());
#         i += 1;
#     }

else_pattern = re.compile(
    r'(\}\s*else\s*\{)\s*\n(\s*)// Add to current literal\s*\n\2current_literal\.push\(content\[i\.\.\]\.chars\(\)\.next\(\)\.unwrap\(\)\);\s*\n\2i\s*\+=\s*1;\s*\n(\s*)\}',
    re.MULTILINE
)

def replace_else(m):
    """Fix else branch to use char width."""
    open_brace = m.group(1)
    indent = m.group(2)
    closing_indent = m.group(3)
    
    new_body = f'{open_brace}\n{indent}// Add to current literal\n{indent}let ch = content[i..].chars().next().unwrap();\n{indent}current_literal.push(ch);\n{indent}i += ch.len_utf8();\n{closing_indent}}}'
    return new_body

new_func_str2, count2 = else_pattern.subn(replace_else, new_func_str if count > 0 else func_str)
if count2 > 0:
    print(f"Replaced {count2} else branch(es)")

# Now we need to also fix the $(...) and ${...} scanning loops
# They have the pattern:
#     while i < content.len() && paren_count > 0 {
#         match content[i..].chars().next() {
#             ...
#         }
#         i += 1;
#     }

# Let's handle these more carefully - find multi-statement while loops with i += 1;
# Look for 'while ... {' followed by multiple lines then 'i += 1;'
multi_while_pattern = re.compile(
    r'(while\s+i\s*<\s*content\.len\(\)\s*&&\s*[^{]+\{)\n(.*?)\n(\s*)i\s*\+=\s*1;\s*\n(\s*)\}',
    re.DOTALL
)

# For multi-statement while loops, we need to replace the i += 1; with char-width advancing
# but within the existing body. Let's find and fix them.

# Simple approach: find all 'i += 1;' that are inside a while loop body
# by checking if there's a 'while' keyword between this line and the last '{'
lines = func_str.split('\n')
result_lines = list(lines)  # copy
changes_made = 0

for idx, line in enumerate(lines):
    stripped = line.strip()
    if stripped == 'i += 1;':
        # Check context - look backwards for a 'while' keyword before an unmatched '{'
        brace_depth = 0
        found_while = False
        for j in range(idx, max(idx - 15, -1), -1):
            l = lines[j]
            brace_depth += l.count('{') - l.count('}')
            if 'while' in l and 'content.len()' in l:
                found_while = True
                break
            if brace_depth > 0:
                # We've gone back past the enclosing block
                break
        
        if found_while:
            # This i += 1; is inside a while loop - fix it
            replacement = 'let ch = content[i..].chars().next().unwrap_or(\'?\');\n                    i += ch.len_utf8();'
            old_line = line
            new_line = line.replace('i += 1;', 'i += ch.len_utf8();')
            # We'll handle this differently
            
# Actually, let's just use a simpler approach: replace ALL 'i += 1;' with proper char stepping
# when they appear in certain contexts.

# The simplest safe approach: replace 'i += 1;' with a macro-like expansion that properly 
# handles multi-byte chars. But Rust doesn't allow macro expansion here.

# Let me just fix the specific patterns I know are problematic:
# 1. The else branches (already done if count2 > 0)
# 2. The scanning while loops (already done if count > 0)

# If neither worked, let me check what the file actually contains
if count == 0 and count2 == 0:
    print("\nTrying direct byte-level fixes...")
    
    # Find all 'i += 1;' occurrences
    pattern_bytes = b'i += 1;'
    pos = 0
    occurrences = []
    while True:
        idx = data.find(pattern_bytes, pos)
        if idx < 0:
            break
        if fn_start <= idx < fn_end:
            occurrences.append(idx)
        pos = idx + 1
    
    print(f"Found {len(occurrences)} 'i += 1;' in function")
    
    # For each occurrence, check if it's in a while loop or else branch
    for idx in occurrences:
        # Get context (previous 500 bytes)
        context_start = max(fn_start, idx - 500)
        context = data[context_start:idx].decode('utf-8', errors='replace')
        
        # Check if this is in a scanning while loop
        lines_before = context.split('\n')
        # Look backwards for 'while' in the last 10 lines
        found_while = False
        found_else = False
        for line in reversed(lines_before[-10:]):
            if 'while' in line and 'content.len()' in line:
                found_while = True
                break
            if '} else {' in line:
                found_else = True
                break
        
        flag = 'WHILE' if found_while else ('ELSE' if found_else else 'OTHER')
        print(f"  Byte {idx}: {flag}")

print("\nDone")

#!/bin/bash
# Regression test: heredoc with Python/JS code containing braces, and other delimiters
f=test.py
cat > "$f" << 'PYEOF'
def foo():
    if True:
        return {"key": "value"}
PYEOF
printf 'wrote %d bytes to %s\n' $(wc -c < "$f") "$f"

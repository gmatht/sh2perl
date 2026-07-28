#!/bin/bash
# Regression test: heredoc with Python/JS code containing braces, and other delimiters
cat > /tmp/test.py << 'PYEOF'
def foo():
    if True:
        return {"key": "value"}
PYEOF
echo "heredoc done"

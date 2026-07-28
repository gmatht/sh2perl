#!/bin/bash
# Minimal sample: heredoc with single quotes creates spanning strings in shell lexer
cat > /tmp/x.py << 'EOF'
x = re.search(r'`([^`]+)`', line)
if x == '\'': pass
EOF
echo "ok"

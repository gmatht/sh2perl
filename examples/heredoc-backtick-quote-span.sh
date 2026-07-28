#!/bin/bash
# Heredoc with single quotes and backticks that create spanning strings
cat > /tmp/x.py << 'EOF'
import re
func_match = re.search(r'`([^`]+)`', line)
if x == '\'': pass
EOF
echo "done"

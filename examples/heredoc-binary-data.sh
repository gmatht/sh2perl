#!/bin/bash
# Script with binary data after heredoc (self-extracting-archive emulation)
# Defaults the interpreter arg so the harness can run it bare.
INTERPRETER_UNDER_TEST="$(command -v "${1:-python3}")"
if [ -z "${INTERPRETER_UNDER_TEST}" ]; then
    echo "Interpreter must be the command line argument."
    exit 4
fi
"${INTERPRETER_UNDER_TEST}" -E - <<'END_OF_PYTHON'
print('Hello from embedded Python')
END_OF_PYTHON

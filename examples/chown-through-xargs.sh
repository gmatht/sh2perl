#!/bin/sh
# chown via xargs - generates open3() with builtin 'chown' as first arg
# Uses echo to avoid actually running
echo "/tmp/test" | xargs -0 --no-run-if-empty echo chown -c root:root -- /tmp/test

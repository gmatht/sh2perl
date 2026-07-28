config=$(echo "#!/bin/sh
exec '$(which cmd)' --option \"\$@\"" > /tmp/config.sh)

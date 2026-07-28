echo "#!/bin/sh
exec '$(echo test)' --flag \"\$@\"" > /tmp/output.sh

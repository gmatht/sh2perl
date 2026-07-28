prepare_gpg_home() {
    echo "#!/bin/sh
exec '$(echo test)' --flag \"\$@\"" > /tmp/output.sh
}

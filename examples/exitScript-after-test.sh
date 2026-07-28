if [[ ! -z $(apt-cache search ^libssl) ]]; then
    exitScript 1
fi

files=(`ls -1 *.sh examples/*.sh 2>/dev/null`); echo "Shell scripts found: ${#files[@]}"; for file in "${files[@]}"; do echo "  - $file"; done

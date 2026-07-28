#!/bin/bash
# Shell keywords as arguments to commands
awk '{for(i=1;i<=NF;i++) if($i ~ /pattern/) print $i}'
echo "done"

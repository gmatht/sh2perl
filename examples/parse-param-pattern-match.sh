#!/bin/sh
# ${p##*[/\\]} and similar pattern-based parameter expansions
p=/opt/tool/git-tool
basename=${p##*[/\\]}
echo "${basename#git-}"

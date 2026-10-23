#!/bin/bash

# 26. Nested braces inside comma alternatives
# A brace level's comma-separated alternatives may each contain a nested
# brace; the nested group is a cartesian FACTOR of that alternative, not
# another sibling alternative (so the surrounding text must be repeated
# for every expanded value).
echo a{x/{p,q},y}z
echo {a,{b,c}}d
echo {src/{main,test},docs}/{a,b}
echo file.{txt,{bak,orig}}
echo pre{x,y}post

# A brace word is still ONE argv word: text before the brace on the SAME
# word is the brace's prefix (`source.{bat,c}`), while a separate word
# stays separate (`-p a/x a/y`, never `-pa/x`).
echo source.{bat,c}
echo -n pre; echo {x,y}post

echo "done: $?"

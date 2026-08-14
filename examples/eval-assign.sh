# eval of an assignment string (no arithmetic — bash assigns the literal
# text "5+1").
x=5
eval "y=$x+1"
echo "y=$y"

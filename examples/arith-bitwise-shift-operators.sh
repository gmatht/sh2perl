# arith bitwise & | ^ << >> ~ (the runtime evaluator was stripped of
# these — a syntax error silently became 0).
echo $(( (5+3) & 12 ))
echo $(( 1 << 4 ))
echo $(( 32 >> 2 ))
echo $(( 7 | 8 ))
echo $(( 12 ^ 5 ))
echo $(( ~0 ))

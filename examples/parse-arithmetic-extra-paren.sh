# $((...)) arithmetic expansion with extra grouping parentheses
# The lexer's ArithmeticEvalClose greedily matches )),
# consuming a ) that belongs to an inner group.
x=$(((a + b) / (c + d)))

#!/bin/bash
# Test: sh2perl can parse hex number tokens (0x...) as command arguments.
# The lexer tokenizes 0x-prefixed strings as HexNumber tokens.
# parse_word_no_newline_skip must include HexNumber in its combining loop
# to avoid an infinite loop where the token is never consumed.
echo 0x1234
apt-key adv --recv-keys 0xA236C58F409091A18ACA53CBEBFF6B99D9B78493

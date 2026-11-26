# Every printf here has a literal format the renderer can decompose at
# render time, so the emitted program must NOT call the __sh_printf
# interpreter — the census pins pfx>0 / pfi=0 / pfl=0 for this fixture.
printf 'name=%s\n' alice
printf '%s-%s\t%s\n' a b c
printf '100%%\n'
printf 'braces {a} and {b} stay literal\n'

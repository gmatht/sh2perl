# The format is assembled at run time (an interpolation of a variable), so
# the site cannot be decomposed at render time: `__sh_printf` must still be
# EMITTED and CALLED for this fixture (pfi=1). A future over-eager
# specialization fails here instead of silently changing the output.
piece='%s'
fmt="$piece/$piece\n"
printf "$fmt" a b

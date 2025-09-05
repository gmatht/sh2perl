yes Line:LINE | head -n100 | while read L; do i=$((i+1)); echo $L | sed s/LINE/$i/ ; done

#Avoid arrays, use a line by line pipeline rather than buffered.
#PERL_MUST_NOT_CONTAIN: @

#Only use basename and main_exit_code if actually needed.
#PERL_MUST_NOT_CONTAIN: Basename
#PERL_MUST_NOT_CONTAIN: main_exit_code

#Not sure why this would appear, but it did
#PERL_MUST_NOT_CONTAIN: $lines=$L

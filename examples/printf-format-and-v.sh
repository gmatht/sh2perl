# printf: %b %s %d formats, \n in format text, and printf -v NAME
# (previously the top-level output was dropped and the format leaked).
printf "%s %d\n" "hi" 42
printf "%b" "a\tb\n"
printf -v name "%s-%s" "foo" "bar"
echo "name=$name"

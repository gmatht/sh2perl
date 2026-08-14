# `.` sourcing must share state with the sourcing script (bug: the
# sourced lib's assignments vanish — `shared=` instead of `shared=secret`).
echo 'shared_val=secret' > lib_tmp.sh
. ./lib_tmp.sh
echo "shared=$shared_val"
rm -f lib_tmp.sh

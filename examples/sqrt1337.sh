for i in `seq 1 10000`
do
	if echo $((i*i)) | grep 1337 > /dev/null 2> /dev/null
	then 
		echo $i
	fi
done

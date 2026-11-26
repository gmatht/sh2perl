rm /no/such/file
if [ $? -eq 0 ]; then echo ok; else echo bad; fi

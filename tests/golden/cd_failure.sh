cd /no/such/dir
if [ $? -eq 0 ]; then echo ok; else echo bad; fi

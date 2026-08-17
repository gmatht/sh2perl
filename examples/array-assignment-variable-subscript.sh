# Array assignment with variable subscript expansion
FilesystemOptions=(a b c)
i=1
echo ${FilesystemOptions[(2*$i)-1]}

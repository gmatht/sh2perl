print 'Working Directory:';
system('pwd');

print 'Files: ';
my $ls_output = `ls`;
print $ls_output;

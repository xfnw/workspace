	move SP, 0
	push 0
	sys $306

again:
	move B, A
	mod B, 10
	add B, $30
	push B
	div A, 10
	bnze A, again
	
	move A, SP
	sys $307
	halt

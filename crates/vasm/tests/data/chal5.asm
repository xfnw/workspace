	move X, res
	sys $308

	move [X++], A
again:
	move B, 1
	and B, A
	bnze B, odd

	div A, 2

	jump next
odd:
	mul A, 3
	inc A

next:
	move [X++], A
	bnze A, again

	move A, res
	sys $309
	halt

res:

	sys $300	; request challenge

	mul A, 9
	div A, 5
	add A, 32

	sys $301	; send answer
	halt

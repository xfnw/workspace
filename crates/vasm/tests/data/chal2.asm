	jump start

num1:	resw 2
num2:	resw 2

start:
	move A, num1
	move X, A
	move Y, num2

	; no reason to do this over move B, 0 since immediate 0
	; does not take an extra word in vm16. i just want more
	; test code coverage :3
	xor B, B

	sys $302
	move A, X	; smh syscall overwriting my shit

	call addh
	call addh

	sys $303
	halt

addh:
	add [X], B
	addc [X++], [Y++]
	ret

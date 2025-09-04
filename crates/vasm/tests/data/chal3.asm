jump start

x1:	dw 0
x2:	dw 0
x3:	dw 0
y1:	dw 0
y2:	dw 0
y3:	dw 0

start:
	move A, x1
	move X, A
	sys $304	; A gets a value of 1

	call doot
	call doot
	call doot

	sys $305
	halt

doot:
	move B, [X+3]
	move C, [X++]
	
	skgt B, C
	xchg B, C
	nop

	add A, B
	sub A, C
	ret

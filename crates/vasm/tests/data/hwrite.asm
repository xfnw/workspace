	move A, msg	; text to send
	sys 0		; write to tele
	move A, cmsg	; text to send
	sys 0		; write to tele
	move A, hmsg	; text to send
	sys $25		; write to chat
	halt

msg:	dw "now with string support!", 0
cmsg:	dw c"and packed strings too!", 0
hmsg:	dw 'h', 0

# vulpine's vm16 assembler
i did not like the syntax of the in game assembler, so i made my own

notable features include:
- a more intel-ish syntax
- automatically detects `sk{ne,eq,lt,gt}` skipping into the middle
  of an instruction
- somewhat cursed parsing that does not require newlines anywhere
  except after line comments
- fully relocatable output (this does mean using a label absolutely is
  unsupported, use relative addressing instead, eg `move X, mylabel`
  `move A, [X]` instead of `move A, [mylabel]`)
- no directives or macros (pipe your assembly through m4 if you want)

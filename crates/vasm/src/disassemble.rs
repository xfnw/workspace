use crate::repr::{Instruction, Instructions};

fn disassemble_one(c: &[u16]) -> Option<(Instruction, &[u16])> {
    let (&f, rest) = c.split_first()?;
    let instr = f & (((1 << 6) - 1) << 10);
    let flags = f & ((1 << 10) - 1);

    match instr {
        0x0000 => (flags == 0).then_some((Instruction::Nop, rest)),
        0x1800 => (flags == 0).then_some((Instruction::Ret, rest)),
        0x1c00 => (flags == 0).then_some((Instruction::Halt, rest)),
        _ => None,
    }
    .or(Some((Instruction::Dw(vec![f]), rest)))
}

pub fn disassemble(bytes: &[u16]) -> Instructions {
    let mut out = vec![];
    let mut cursor = bytes;

    while let Some((ins, rest)) = disassemble_one(cursor) {
        out.push(ins);
        cursor = rest;
    }

    Instructions(out)
}

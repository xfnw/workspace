#![allow(clippy::cast_possible_wrap)]

use crate::repr::{
    Const, Dst, Immediate, Instruction, Instructions, LabelOffset, MemoryAddress, Offset, Operand,
    Opnd, Src, TwoOpnd,
};

fn left_opnd(v: u16, c: &[u16]) -> Option<(Operand, &[u16])> {
    let flags = v & (((1 << 5) - 1) << 5);
    Some((
        match flags {
            0x0000 => Operand::A,
            0x0020 => Operand::B,
            0x0040 => Operand::C,
            0x0060 => Operand::D,
            0x0080 => Operand::X,
            0x00a0 => Operand::Y,
            0x00c0 => Operand::PC,
            0x00e0 => Operand::SP,
            0x0100 => Operand::AtX,
            0x0120 => Operand::AtY,
            0x0140 => Operand::AtXInc,
            0x0160 => Operand::AtYInc,
            0x0180 => Operand::Immed0,
            0x01a0 => Operand::Immed1,
            0x0200 => {
                return c
                    .split_first()
                    .and_then(|(&f, r)| Some((Operand::Immediate(Immediate::new(f)?), r)));
            }
            0x0220 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::Mem(MemoryAddress::new(f)), r));
            }
            0x0260 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::AtSPn(Offset::new(f as i16)), r));
            }
            0x0280 => {
                return c.split_first().map(|(&f, r)| {
                    (
                        Operand::Rel2(LabelOffset::new(None, Offset::new(f as i16))),
                        r,
                    )
                });
            }
            0x02a0 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::AtXn(Offset::new(f as i16)), r));
            }
            0x02c0 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::AtYn(Offset::new(f as i16)), r));
            }
            0x02e0 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::SPn(Offset::new(f as i16)), r));
            }
            _ => return None,
        },
        c,
    ))
}

fn right_opnd(v: u16, c: &[u16]) -> Option<(Operand, &[u16])> {
    let flags = v & ((1 << 5) - 1);
    Some((
        match flags {
            0x0000 => Operand::A,
            0x0001 => Operand::B,
            0x0002 => Operand::C,
            0x0003 => Operand::D,
            0x0004 => Operand::X,
            0x0005 => Operand::Y,
            0x0006 => Operand::PC,
            0x0007 => Operand::SP,
            0x0008 => Operand::AtX,
            0x0009 => Operand::AtY,
            0x000a => Operand::AtXInc,
            0x000b => Operand::AtYInc,
            0x000c => Operand::Immed0,
            0x000d => Operand::Immed1,
            0x0010 => {
                return c
                    .split_first()
                    .and_then(|(&f, r)| Some((Operand::Immediate(Immediate::new(f)?), r)));
            }
            0x0011 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::Mem(MemoryAddress::new(f)), r));
            }
            0x0013 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::AtSPn(Offset::new(f as i16)), r));
            }
            0x0014 => {
                return c.split_first().map(|(&f, r)| {
                    (
                        Operand::Rel2(LabelOffset::new(None, Offset::new(f as i16))),
                        r,
                    )
                });
            }
            0x0015 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::AtXn(Offset::new(f as i16)), r));
            }
            0x0016 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::AtYn(Offset::new(f as i16)), r));
            }
            0x0017 => {
                return c
                    .split_first()
                    .map(|(&f, r)| (Operand::SPn(Offset::new(f as i16)), r));
            }
            _ => return None,
        },
        c,
    ))
}

fn disassemble_instruction(f: u16, rest: &[u16]) -> Option<(Instruction, &[u16])> {
    let instr = f & (((1 << 6) - 1) << 10);
    let flags = f & ((1 << 10) - 1);

    macro_rules! opnd {
        ($inst:ident, Src) => {{
            let (operand, rest) = left_opnd(flags, rest)?;
            (Instruction::$inst(Opnd::<Src>::new(operand)), rest)
        }};
        ($inst:ident, $kind:ident) => {{
            let (operand, rest) = left_opnd(flags, rest)?;
            (Instruction::$inst(Opnd::<$kind>::new(operand).ok()?), rest)
        }};
        ($inst:ident, Src, Src) => {{
            let (o1, rest) = left_opnd(flags, rest)?;
            let (o2, rest) = right_opnd(flags, rest)?;
            (Instruction::$inst(TwoOpnd::<Src, Src>::new(o1, o2)), rest)
        }};
        ($inst:ident, $left:ident, $right:ident) => {{
            let (o1, rest) = left_opnd(flags, rest)?;
            let (o2, rest) = right_opnd(flags, rest)?;
            (
                Instruction::$inst(TwoOpnd::<$left, $right>::new(o1, o2).ok()?),
                rest,
            )
        }};
    }

    Some(match instr {
        0x0000 => return (flags == 0).then_some((Instruction::Nop, rest)),
        0x0400 => (Instruction::Brk(Const::new(flags).unwrap()), rest),
        0x0800 => (Instruction::Sys(Const::new(flags).unwrap()), rest),
        0x1000 => opnd!(Jump, Src),
        0x1400 => opnd!(Call, Src),
        0x1800 => return (flags == 0).then_some((Instruction::Ret, rest)),
        0x1c00 => return (flags == 0).then_some((Instruction::Halt, rest)),
        0x2000 => opnd!(Move, Dst, Src),
        0x2400 => opnd!(Xchg, Dst, Dst),
        0x2800 => opnd!(Inc, Dst),
        0x2c00 => opnd!(Dec, Dst),
        0x3000 => opnd!(Add, Dst, Src),
        0x3400 => opnd!(Sub, Dst, Src),
        0x3800 => opnd!(Mul, Dst, Src),
        0x3c00 => opnd!(Div, Dst, Src),
        0x4000 => opnd!(And, Dst, Src),
        0x4400 => opnd!(Or, Dst, Src),
        0x4800 => opnd!(Xor, Dst, Src),
        0x4c00 => opnd!(Not, Dst),
        0x5000 => opnd!(Bnze, Src, Src),
        0x5400 => opnd!(Bze, Src, Src),
        0x5800 => opnd!(Bpos, Src, Src),
        0x5c00 => opnd!(Bneg, Src, Src),
        0x6000 => opnd!(In, Dst, Src),
        0x6400 => opnd!(Out, Src, Src),
        0x6800 => opnd!(Push, Src),
        0x6c00 => opnd!(Pop, Dst),
        0x7000 => opnd!(Swap, Dst),
        0x7400 => opnd!(Dbnz, Dst, Src),
        0x7800 => opnd!(Mod, Dst, Src),
        0x7c00 => opnd!(Shl, Dst, Src),
        0x8000 => opnd!(Shr, Dst, Src),
        0x8400 => opnd!(Addc, Dst, Src),
        0x8800 => opnd!(Mulc, Dst, Src),
        0x8c00 => opnd!(Skne, Src, Src),
        0x9000 => opnd!(Skeq, Src, Src),
        0x9400 => opnd!(Sklt, Src, Src),
        0x9800 => opnd!(Skgt, Src, Src),
        0x9c00 => opnd!(Msb, Dst, Src),
        _ => return None,
    })
}

fn disassemble_one(c: &[u16]) -> Option<(Instruction, &[u16])> {
    let (&f, rest) = c.split_first()?;
    disassemble_instruction(f, rest).or(Some((Instruction::Dw(vec![f]), rest)))
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

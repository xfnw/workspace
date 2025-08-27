use crate::repr::{Instruction, Opnd, Opnd1, Opnd2, TwoOpnd};

use super::repr::Operand;

/// helper trait for calculating relative offsets
pub trait AssSize {
    /// the number of words taken up by machine code after being
    /// assembled
    fn size(&self) -> usize;
}

impl AssSize for Operand {
    fn size(&self) -> usize {
        match self {
            Self::A
            | Self::B
            | Self::C
            | Self::D
            | Self::X
            | Self::Y
            | Self::PC
            | Self::SP
            | Self::AtX
            | Self::AtY
            | Self::AtXInc
            | Self::AtYInc
            | Self::Immed0
            | Self::Immed1 => 0,
            Self::Immediate(_)
            | Self::Mem(_)
            | Self::AtSPn(_)
            | Self::Rel2(_)
            | Self::AtXn(_)
            | Self::AtYn(_)
            | Self::SPn(_) => 1,
        }
    }
}

impl AssSize for Opnd1 {
    fn size(&self) -> usize {
        self.value().size()
    }
}

impl AssSize for Opnd2 {
    fn size(&self) -> usize {
        self.value().size()
    }
}

impl<T> AssSize for Opnd<T> {
    fn size(&self) -> usize {
        self.left().size()
    }
}

impl<L, R> AssSize for TwoOpnd<L, R> {
    fn size(&self) -> usize {
        self.left().size() + self.right().size()
    }
}

impl AssSize for Instruction {
    fn size(&self) -> usize {
        match self {
            Self::Nop | Self::Ret | Self::Halt | Self::Brk(_) | Self::Sys(_) => 1,
            Self::Jump(i) | Self::Call(i) | Self::Push(i) => 1 + i.size(),
            Self::Inc(i) | Self::Dec(i) | Self::Not(i) | Self::Pop(i) | Self::Swap(i) => {
                1 + i.size()
            }
            Self::Xchg(i) => 1 + i.size(),
            Self::Move(i)
            | Self::Add(i)
            | Self::Sub(i)
            | Self::Mul(i)
            | Self::Div(i)
            | Self::And(i)
            | Self::Or(i)
            | Self::Xor(i)
            | Self::In(i)
            | Self::Dbnz(i)
            | Self::Mod(i)
            | Self::Shl(i)
            | Self::Shr(i)
            | Self::Addc(i)
            | Self::Mulc(i)
            | Self::Msb(i) => 1 + i.size(),
            Self::Bnze(i)
            | Self::Bze(i)
            | Self::Bpos(i)
            | Self::Bneg(i)
            | Self::Out(i)
            | Self::Skne(i)
            | Self::Skeq(i)
            | Self::Sklt(i)
            | Self::Skgt(i) => 1 + i.size(),
            Self::LabelDef(_) | Self::Comment(_) => 0,
            Self::Dw(v) => v.len(),
        }
    }
}

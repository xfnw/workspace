use crate::repr::{Instruction, Operand, Opnd, Opnd1, Opnd2, TwoOpnd};
use std::collections::BTreeMap;

/// helper trait for calculating relative offsets
trait AssSize {
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

#[derive(Debug, foxerror::FoxError)]
pub enum Error {
    /// likely incorrect usage of sk* instructions will misalign
    ///
    /// if you're intentionally making a program that has different
    /// behavior than the assembly suggests, consider using dw to make
    /// it more obvious there is more going on.
    SkMisalignment(Instruction),
    /// label defined multiple times
    DuplicateLabel(String),
    /// instruction takes up more than 65535 words!?
    InstructionTooBig(Instruction),
    /// your code is too big to fit in vm16's entire memory space
    CodeTooLong,
    /// could not find label
    LabelNotFound(String),
}

fn label_offset(label: &str, loc: u16, labels: &BTreeMap<String, u16>) -> Result<u16, Error> {
    if let Some(l) = labels.get(label) {
        // TODO: explicitly use wrapping subtraction
        return Ok(l - loc);
    }

    Err(Error::LabelNotFound(label.to_string()))
}

enum Extra {
    None,
    One(u16),
    Two(u16, u16),
}

trait AssPart {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error>;
}

impl AssPart for Opnd1 {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error> {
        todo!()
    }
}

impl AssPart for Opnd2 {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error> {
        todo!()
    }
}

fn assemble_one(
    loc: u16,
    instruction: &Instruction,
    labels: &BTreeMap<String, u16>,
) -> Result<Vec<u16>, Error> {
    todo!()
}

pub fn assemble(rep: Vec<Instruction>) -> Result<Vec<u16>, Error> {
    let mut labels = BTreeMap::new();
    let loc = rep
        .into_iter()
        .scan((0u16, None), |(statepos, skt), i| {
            let pos = *statepos;
            let Ok(size) = u16::try_from(i.size()) else {
                return Some(Err(Error::InstructionTooBig(i)));
            };
            *statepos = if let Some(new) = (*statepos).checked_add(size) {
                new
            } else {
                return Some(Err(Error::CodeTooLong));
            };

            if let Some(skt) = skt
                && pos < *skt
                && *statepos > *skt
                && !matches!(i, Instruction::Dw(_))
            {
                return Some(Err(Error::SkMisalignment(i)));
            }

            match i {
                Instruction::LabelDef(ref def) => {
                    if labels.insert(def.clone(), pos).is_some() {
                        return Some(Err(Error::DuplicateLabel(def.clone())));
                    }
                }
                Instruction::Skne(_)
                | Instruction::Skeq(_)
                | Instruction::Sklt(_)
                | Instruction::Skgt(_) => {
                    *skt = Some(*statepos + 2);
                }
                _ => (),
            }

            Some(Ok((pos, i)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut out = vec![];

    for (l, ins) in loc {
        assert_eq!(
            l as usize,
            out.len(),
            "instruction before {ins} has incorrect size"
        );

        out.append(&mut assemble_one(l, &ins, &labels)?);
    }

    Ok(out)
}

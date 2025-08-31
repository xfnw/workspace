use crate::repr::{Const, Instruction, Operand, Opnd, Opnd1, Opnd2, TwoOpnd};
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
        return Ok(l.wrapping_sub(loc));
    }

    Err(Error::LabelNotFound(label.to_string()))
}

enum Extra {
    None,
    One(u16),
    Two(u16, u16),
}

fn extra(operand: &Operand, loc: u16, labels: &BTreeMap<String, u16>) -> Result<Extra, Error> {
    Ok(match operand {
        Operand::A
        | Operand::B
        | Operand::C
        | Operand::D
        | Operand::X
        | Operand::Y
        | Operand::PC
        | Operand::SP
        | Operand::AtX
        | Operand::AtY
        | Operand::AtXInc
        | Operand::AtYInc
        | Operand::Immed0
        | Operand::Immed1 => Extra::None,
        Operand::Immediate(immed) => Extra::One(immed.value()),
        Operand::Mem(mem) => Extra::One(mem.value()),
        Operand::AtSPn(off) | Operand::AtXn(off) | Operand::AtYn(off) | Operand::SPn(off) =>
        {
            #[allow(clippy::cast_sign_loss)]
            Extra::One(off.value() as u16)
        }
        Operand::Rel2(rel) => Extra::One(
            if let Some(label) = rel.name() {
                label_offset(label, loc, labels)?
            } else {
                0
            }
            .wrapping_add_signed(rel.offset().value()),
        ),
    })
}

trait AssPart {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error>;
}

impl AssPart for Opnd1 {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error> {
        let opnd = self.value();
        let flags = match opnd {
            Operand::A => 0x0000,
            Operand::B => 0x0020,
            Operand::C => 0x0040,
            Operand::D => 0x0060,
            Operand::X => 0x0080,
            Operand::Y => 0x00a0,
            Operand::PC => 0x00c0,
            Operand::SP => 0x00e0,
            Operand::AtX => 0x0100,
            Operand::AtY => 0x0120,
            Operand::AtXInc => 0x0140,
            Operand::AtYInc => 0x0160,
            Operand::Immed0 => 0x0180,
            Operand::Immed1 => 0x01a0,
            Operand::Immediate(_) => 0x0200,
            Operand::Mem(_) => 0x0220,
            Operand::AtSPn(_) => 0x0260,
            Operand::Rel2(_) => 0x0280,
            Operand::AtXn(_) => 0x02a0,
            Operand::AtYn(_) => 0x02c0,
            Operand::SPn(_) => 0x02e0,
        };
        Ok((flags, extra(opnd, loc, labels)?))
    }
}

impl AssPart for Opnd2 {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error> {
        let opnd = self.value();
        let flags = match opnd {
            Operand::A => 0x0000,
            Operand::B => 0x0001,
            Operand::C => 0x0002,
            Operand::D => 0x0003,
            Operand::X => 0x0004,
            Operand::Y => 0x0005,
            Operand::PC => 0x0006,
            Operand::SP => 0x0007,
            Operand::AtX => 0x0008,
            Operand::AtY => 0x0009,
            Operand::AtXInc => 0x000a,
            Operand::AtYInc => 0x000b,
            Operand::Immed0 => 0x000c,
            Operand::Immed1 => 0x000d,
            Operand::Immediate(_) => 0x0010,
            Operand::Mem(_) => 0x0011,
            Operand::AtSPn(_) => 0x0013,
            Operand::Rel2(_) => 0x0014,
            Operand::AtXn(_) => 0x0015,
            Operand::AtYn(_) => 0x0016,
            Operand::SPn(_) => 0x0017,
        };
        Ok((flags, extra(opnd, loc, labels)?))
    }
}

impl<T> AssPart for Opnd<T> {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error> {
        self.left().part(loc, labels)
    }
}

impl<L, R> AssPart for TwoOpnd<L, R> {
    fn part(&self, loc: u16, labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error> {
        let (ln, lext) = self.left().part(loc, labels)?;
        let (rn, rext) = self.right().part(loc, labels)?;
        assert!(ln & rn == 0 && ln + rn < 1 << 10, "nonsensical flags");
        let ext = match (lext, rext) {
            (Extra::None, Extra::None) => Extra::None,
            (Extra::One(e), Extra::None) | (Extra::None, Extra::One(e)) => Extra::One(e),
            (Extra::One(l), Extra::One(r)) => Extra::Two(l, r),
            _ => panic!("single operand should not have more than one extra word"),
        };
        Ok((ln + rn, ext))
    }
}

impl AssPart for Const {
    fn part(&self, _loc: u16, _labels: &BTreeMap<String, u16>) -> Result<(u16, Extra), Error> {
        Ok((self.value(), Extra::None))
    }
}

#[allow(clippy::too_many_lines)]
fn assemble_one(
    loc: u16,
    instruction: &Instruction,
    labels: &BTreeMap<String, u16>,
) -> Result<Vec<u16>, Error> {
    // FIXME: generate this mess with a macro or something
    let (flags, extra) = match instruction {
        Instruction::Nop => (0x0000, Extra::None),
        Instruction::Brk(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x0400 + f, e)
        }
        Instruction::Sys(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x0800 + f, e)
        }
        Instruction::Jump(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x1000 + f, e)
        }
        Instruction::Call(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x1400 + f, e)
        }
        Instruction::Ret => (0x1800, Extra::None),
        Instruction::Halt => (0x1C00, Extra::None),
        Instruction::Move(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x2000 + f, e)
        }
        Instruction::Xchg(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x2400 + f, e)
        }
        Instruction::Inc(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x2800 + f, e)
        }
        Instruction::Dec(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x2c00 + f, e)
        }
        Instruction::Add(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x3000 + f, e)
        }
        Instruction::Sub(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x3400 + f, e)
        }
        Instruction::Mul(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x3800 + f, e)
        }
        Instruction::Div(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x3C00 + f, e)
        }
        Instruction::And(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x4000 + f, e)
        }
        Instruction::Or(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x4400 + f, e)
        }
        Instruction::Xor(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x4800 + f, e)
        }
        Instruction::Not(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x4c00 + f, e)
        }
        Instruction::Bnze(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x5000 + f, e)
        }
        Instruction::Bze(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x5400 + f, e)
        }
        Instruction::Bpos(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x5800 + f, e)
        }
        Instruction::Bneg(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x5c00 + f, e)
        }
        Instruction::In(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x6000 + f, e)
        }
        Instruction::Out(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x6400 + f, e)
        }
        Instruction::Push(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x6800 + f, e)
        }
        Instruction::Pop(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x6c00 + f, e)
        }
        Instruction::Swap(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x7000 + f, e)
        }
        Instruction::Dbnz(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x7400 + f, e)
        }
        Instruction::Mod(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x7800 + f, e)
        }
        Instruction::Shl(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x7c00 + f, e)
        }
        Instruction::Shr(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x8000 + f, e)
        }
        Instruction::Addc(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x8400 + f, e)
        }
        Instruction::Mulc(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x8800 + f, e)
        }
        Instruction::Skne(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x8c00 + f, e)
        }
        Instruction::Skeq(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x9000 + f, e)
        }
        Instruction::Sklt(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x9400 + f, e)
        }
        Instruction::Skgt(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x9800 + f, e)
        }
        Instruction::Msb(o) => {
            let (f, e) = o.part(loc, labels)?;
            (0x9c00 + f, e)
        }
        Instruction::LabelDef(_) | Instruction::Comment(_) => return Ok(vec![]),
        Instruction::Dw(v) => return Ok(v.clone()),
    };

    let mut out = vec![flags];
    match extra {
        Extra::None => (),
        Extra::One(o) => out.push(o),
        Extra::Two(one, two) => {
            out.push(one);
            out.push(two);
        }
    }

    Ok(out)
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

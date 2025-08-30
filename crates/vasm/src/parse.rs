use crate::repr::{self, Const, Dst, Instruction, LabelOffset, Operand, Opnd, Src, TwoOpnd};
use nom::{
    Err, IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{alpha1, alphanumeric1, multispace0, one_of, space0},
    combinator::{complete, map, map_res, opt, recognize, value},
    multi::{many0, many1, separated_list1},
    sequence::{delimited, pair, preceded, separated_pair, terminated},
};

#[derive(Debug, PartialEq, Eq)]
pub struct LineContext {
    line: usize,
    snippet: String,
}

impl LineContext {
    fn get_context(slice: &str, subslice: &str) -> Self {
        let snippet = subslice
            .chars()
            .enumerate()
            .map_while(|(n, c)| (n < 10 && c != '\n').then_some(c))
            .collect();
        let target = subslice.as_ptr() as usize - slice.as_ptr() as usize;
        let line = slice
            .as_bytes()
            .iter()
            .take(target)
            .filter(|&b| *b == b'\n')
            .count();

        Self {
            line: line + 1,
            snippet,
        }
    }
}

impl std::fmt::Display for LineContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "on line {} at: {}", self.line, self.snippet)
    }
}

#[derive(Debug, foxerror::FoxError)]
pub enum Error {
    /// could not build internal representation
    #[err(from)]
    Repr(repr::Error),
    /// invalid syntax or value
    #[err(from)]
    Parse(LineContext),
}

fn hexadecimal_value(inp: &str) -> IResult<&str, u16> {
    map_res(
        preceded(
            alt((tag("$"), tag("0x"), tag("0X"))),
            recognize(many1(one_of("0123456789abcdefABCDEF"))),
        ),
        |out| u16::from_str_radix(out, 16),
    )
    .parse(inp)
}

fn decimal_value(inp: &str) -> IResult<&str, u16> {
    map_res(recognize(many1(one_of("0123456789"))), |out: &str| {
        out.parse::<u16>()
    })
    .parse(inp)
}

fn number_value(inp: &str) -> IResult<&str, u16> {
    alt((hexadecimal_value, decimal_value)).parse(inp)
}

fn number_offset(inp: &str) -> IResult<&str, repr::Offset> {
    map_res(
        pair(
            alt((value(false, tag("+")), value(true, tag("-")))),
            number_value,
        ),
        |(neg, num)| {
            num.try_into()
                .map(|n| repr::Offset::new(if neg { 0 - n } else { n }))
        },
    )
    .parse(inp)
}

fn label_name(inp: &str) -> IResult<&str, &str> {
    recognize(pair(
        alt((alpha1, tag("_"))),
        many0(alt((alphanumeric1, tag("_")))),
    ))
    .parse(inp)
}

fn label_offset(inp: &str) -> IResult<&str, LabelOffset> {
    alt((
        map(pair(opt(label_name), number_offset), |(name, offset)| {
            LabelOffset::new(name.map(str::to_string), offset)
        }),
        map(label_name, |name| {
            LabelOffset::new(Some(name.to_string()), repr::Offset::new(0))
        }),
    ))
    .parse(inp)
}

fn operand(inp: &str) -> IResult<&str, Operand> {
    alt((
        value(Operand::AtX, tag("[X]")),
        value(Operand::AtY, tag("[Y]")),
        value(Operand::AtXInc, tag("[X++]")),
        value(Operand::AtYInc, tag("[Y++]")),
        map(number_value, Operand::new_immediate),
        map(delimited(tag("["), number_value, tag("]")), |v| {
            Operand::Mem(repr::MemoryAddress::new(v))
        }),
        map(
            delimited(tag("[SP"), number_offset, tag("]")),
            Operand::AtSPn,
        ),
        map(delimited(tag("[X"), number_offset, tag("]")), Operand::AtXn),
        map(delimited(tag("[Y"), number_offset, tag("]")), Operand::AtYn),
        map(preceded(tag("SP"), number_offset), Operand::SPn),
        value(Operand::A, tag("A")),
        value(Operand::B, tag("B")),
        value(Operand::C, tag("C")),
        value(Operand::D, tag("D")),
        value(Operand::X, tag("X")),
        value(Operand::Y, tag("Y")),
        value(Operand::PC, tag("PC")),
        value(Operand::SP, tag("SP")),
        map(label_offset, Operand::Rel2),
    ))
    .parse(inp)
}

fn label_def(inp: &str) -> IResult<&str, Instruction> {
    map(terminated(label_name, tag(":")), |l| {
        Instruction::LabelDef(l.to_string())
    })
    .parse(inp)
}

fn one_opnd(inp: &str) -> IResult<&str, Operand> {
    delimited(space0, operand, space0).parse(inp)
}

fn two_opnd(inp: &str) -> IResult<&str, (Operand, Operand)> {
    separated_pair(one_opnd, tag(","), one_opnd).parse(inp)
}

fn string_value(inp: &str) -> IResult<&str, Vec<u16>> {
    map(delimited(tag("\""), is_not("\""), tag("\"")), |s: &str| {
        s.as_bytes().iter().map(|&c| c.into()).collect()
    })
    .parse(inp)
}

fn string_packed(inp: &str) -> IResult<&str, Vec<u16>> {
    map(preceded(tag("c"), string_value), |v| {
        v.chunks(2)
            .map(|p| match p.len() {
                1 => p[0],
                2 => (p[0] << 8) + p[1],
                _ => unreachable!(),
            })
            .collect()
    })
    .parse(inp)
}

fn number_words(inp: &str) -> IResult<&str, Vec<u16>> {
    alt((string_value, string_packed, map(number_value, |v| vec![v]))).parse(inp)
}

#[allow(clippy::too_many_lines)]
fn instruction(inp: &str) -> IResult<&str, Instruction> {
    // cursed alt nesting courtesy of alt only supporting up to 21
    // items per tuple and rust's opaque types for closures making
    // using an array difficult
    // FIXME: generate this mess with a macro or something
    // FIXME: should probably require whitespace after instructions
    // with operands
    alt((
        alt((
            value(Instruction::Nop, tag("nop")),
            map_res(
                preceded(tag("brk"), delimited(space0, number_value, space0)),
                |n| Const::new(n).map(Instruction::Brk),
            ),
            map_res(
                preceded(tag("sys"), delimited(space0, number_value, space0)),
                |n| Const::new(n).map(Instruction::Sys),
            ),
            map(preceded(tag("jump"), one_opnd), |l| {
                Instruction::Jump(Opnd::<Src>::new(l))
            }),
            map(preceded(tag("call"), one_opnd), |l| {
                Instruction::Call(Opnd::<Src>::new(l))
            }),
            value(Instruction::Ret, tag("ret")),
            value(Instruction::Halt, tag("halt")),
            map_res(preceded(tag("move"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Move)
            }),
            map_res(preceded(tag("xchg"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Dst>::new(l, r).map(Instruction::Xchg)
            }),
            map_res(preceded(tag("inc"), one_opnd), |l| {
                Opnd::<Dst>::new(l).map(Instruction::Inc)
            }),
            map_res(preceded(tag("dec"), one_opnd), |l| {
                Opnd::<Dst>::new(l).map(Instruction::Dec)
            }),
            map_res(preceded(tag("add"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Add)
            }),
            map_res(preceded(tag("sub"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Sub)
            }),
            map_res(preceded(tag("mul"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Mul)
            }),
            map_res(preceded(tag("div"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Div)
            }),
            map_res(preceded(tag("and"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::And)
            }),
            map_res(preceded(tag("or"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Or)
            }),
            map_res(preceded(tag("xor"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Xor)
            }),
            map_res(preceded(tag("not"), one_opnd), |l| {
                Opnd::<Dst>::new(l).map(Instruction::Not)
            }),
            map(preceded(tag("bnze"), two_opnd), |(l, r)| {
                Instruction::Bnze(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map(preceded(tag("bze"), two_opnd), |(l, r)| {
                Instruction::Bze(TwoOpnd::<Src, Src>::new(l, r))
            }),
        )),
        alt((
            map(preceded(tag("bpos"), two_opnd), |(l, r)| {
                Instruction::Bpos(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map(preceded(tag("bneg"), two_opnd), |(l, r)| {
                Instruction::Bneg(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map_res(preceded(tag("in"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::In)
            }),
            map(preceded(tag("out"), two_opnd), |(l, r)| {
                Instruction::Out(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map(preceded(tag("push"), one_opnd), |l| {
                Instruction::Push(Opnd::<Src>::new(l))
            }),
            map_res(preceded(tag("pop"), one_opnd), |l| {
                Opnd::<Dst>::new(l).map(Instruction::Pop)
            }),
            map_res(preceded(tag("swap"), one_opnd), |l| {
                Opnd::<Dst>::new(l).map(Instruction::Swap)
            }),
            map_res(preceded(tag("dbnz"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Dbnz)
            }),
            map_res(preceded(tag("mod"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Mod)
            }),
            map_res(preceded(tag("shl"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Shl)
            }),
            map_res(preceded(tag("shr"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Shr)
            }),
            map_res(preceded(tag("addc"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Addc)
            }),
            map_res(preceded(tag("mulc"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Mulc)
            }),
            map(preceded(tag("skne"), two_opnd), |(l, r)| {
                Instruction::Skne(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map(preceded(tag("skeq"), two_opnd), |(l, r)| {
                Instruction::Skeq(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map(preceded(tag("sklt"), two_opnd), |(l, r)| {
                Instruction::Sklt(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map(preceded(tag("skgt"), two_opnd), |(l, r)| {
                Instruction::Skgt(TwoOpnd::<Src, Src>::new(l, r))
            }),
            map_res(preceded(tag("msb"), two_opnd), |(l, r)| {
                TwoOpnd::<Dst, Src>::new(l, r).map(Instruction::Msb)
            }),
            map(
                preceded(
                    tag("dw"),
                    separated_list1(tag(","), delimited(space0, number_words, space0)),
                ),
                |v| Instruction::Dw(v.into_iter().flatten().collect()),
            ),
            map(
                preceded(tag("resw"), delimited(space0, number_value, space0)),
                |n| Instruction::Dw(vec![0; n.into()]),
            ),
        )),
    ))
    .parse(inp)
}

fn comment(inp: &str) -> IResult<&str, Instruction> {
    map(preceded(tag(";"), is_not("\r\n")), |c: &str| {
        Instruction::Comment(c.to_string())
    })
    .parse(inp)
}

fn document(inp: &str) -> IResult<&str, Vec<Instruction>> {
    complete(many0(terminated(
        alt((
            label_def,
            preceded(space0, instruction),
            preceded(space0, comment),
        )),
        multispace0,
    )))
    .parse(inp)
}

pub fn parse(inp: &str) -> Result<Vec<Instruction>, Error> {
    let (tail, out) = document(inp).map_err(|e| {
        let inner = match e {
            Err::Error(i) | Err::Failure(i) => i,
            Err::Incomplete(_) => unreachable!("complete should turn this into Err::Error"),
        };
        LineContext::get_context(inp, inner.input)
    })?;
    // TODO: replace with nom-supreme's final_parser once it supports nom v8
    // it'll also get less useless error messages and map_res_cut
    if !tail.is_empty() {
        return Err(LineContext::get_context(inp, tail).into());
    }
    Ok(out)
}

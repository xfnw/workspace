use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{alpha1, alphanumeric1, char, one_of},
    combinator::{map, map_res, opt, recognize, value},
    multi::{many0, many1},
    sequence::{delimited, pair, preceded, terminated},
};

use crate::repr::{Instruction, LabelOffset, Operand};

use super::repr;

#[derive(Debug, foxerror::FoxError)]
pub enum Error {
    /// could not build internal representation
    #[err(from)]
    Repr(repr::Error),
    /// could not parse
    #[err(from)]
    Parse(nom::Err<nom::error::Error<String>>),
}

fn hexadecimal_value(inp: &str) -> IResult<&str, u16> {
    map_res(
        preceded(tag("$"), recognize(many1(one_of("0123456789abcdefABCDEF")))),
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
        value(Operand::AtXInc, tag("[X]+")),
        value(Operand::AtYInc, tag("[Y]+")),
        value(Operand::AtX, tag("[X]")),
        value(Operand::AtY, tag("[Y]")),
        map(preceded(tag("#"), number_value), Operand::new_immediate),
        map(number_value, |v| Operand::Mem(repr::MemoryAddress::new(v))),
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

fn comment(inp: &str) -> IResult<&str, Instruction> {
    map(preceded(tag(";"), is_not("\r\n")), |c| {
        Instruction::Comment(c.to_string())
    })
    .parse(inp)
}

#[allow(clippy::redundant_closure_for_method_calls)]
pub fn parse(inp: &str) -> Result<Vec<Instruction>, Error> {
    dbg!(operand(inp).map_err(|e| e.to_owned())?);
    todo!()
}

use crate::repr::{
    self, Const, Dst, Instruction, Instructions, LabelOffset, Operand, Opnd, Src, TwoOpnd,
};
use nom::{
    Err, IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{alpha1, alphanumeric1, anychar, multispace0, one_of, space0, space1},
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

#[test]
fn test_line_context() {
    let teststr = "yip\nyap\nyop\nyote\n";
    assert_eq!(
        LineContext::get_context(teststr, &teststr[8..]),
        LineContext {
            line: 3,
            snippet: "yop".to_string()
        }
    );
    assert_eq!(
        LineContext::get_context(teststr, &teststr[12..]),
        LineContext {
            line: 4,
            snippet: "yote".to_string()
        }
    );
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

fn char_value(inp: &str) -> IResult<&str, u16> {
    map(delimited(tag("'"), anychar, tag("'")), |out| out as u16).parse(inp)
}

fn number_value(inp: &str) -> IResult<&str, u16> {
    alt((hexadecimal_value, decimal_value, char_value)).parse(inp)
}

#[test]
fn test_numbers() {
    assert_eq!(number_value("1234"), Ok(("", 1234)));
    assert_eq!(number_value("0xaaaa"), Ok(("", 0xaaaa)));
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

#[test]
fn test_label_offset() {
    assert_eq!(
        label_offset("+1234"),
        Ok(("", LabelOffset::new(None, repr::Offset::new(1234))))
    );
    assert_eq!(
        label_offset("-3456"),
        Ok(("", LabelOffset::new(None, repr::Offset::new(-3456))))
    );
    assert_eq!(
        label_offset("meow+1234"),
        Ok((
            "",
            LabelOffset::new(Some("meow".to_string()), repr::Offset::new(1234))
        ))
    );
    assert_eq!(
        label_offset("meow-3456"),
        Ok((
            "",
            LabelOffset::new(Some("meow".to_string()), repr::Offset::new(-3456))
        ))
    );
    assert_eq!(
        label_offset("meow"),
        Ok((
            "",
            LabelOffset::new(Some("meow".to_string()), repr::Offset::new(0))
        ))
    );
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

#[test]
fn test_label_def() {
    assert_eq!(
        label_def("meow: woof"),
        Ok((" woof", Instruction::LabelDef("meow".to_string())))
    );
}

fn one_opnd(inp: &str) -> IResult<&str, Operand> {
    delimited(space1, operand, space0).parse(inp)
}

fn two_opnd(inp: &str) -> IResult<&str, (Operand, Operand)> {
    separated_pair(one_opnd, tag(","), delimited(space0, operand, space0)).parse(inp)
}

#[test]
fn test_operands() {
    assert_eq!(one_opnd(" A"), Ok(("", Operand::A)));
    assert_eq!(two_opnd(" B,C"), Ok(("", (Operand::B, Operand::C))));
    assert_eq!(two_opnd(" D, PC"), Ok(("", (Operand::D, Operand::PC))));
    assert_eq!(two_opnd(" SP, [X]"), Ok(("", (Operand::SP, Operand::AtX))));
    assert_eq!(
        two_opnd(" [Y], [X++]"),
        Ok(("", (Operand::AtY, Operand::AtXInc)))
    );
    assert_eq!(
        two_opnd(" [Y++], 0"),
        Ok(("", (Operand::AtYInc, Operand::Immed0)))
    );
    assert_eq!(
        two_opnd(" 1, 2"),
        Ok((
            "",
            (
                Operand::Immed1,
                Operand::Immediate(repr::Immediate::new(2).unwrap())
            )
        ))
    );
    assert_eq!(
        two_opnd(" [1234], meow"),
        Ok((
            "",
            (
                Operand::Mem(repr::MemoryAddress::new(1234)),
                Operand::Rel2(LabelOffset::new(
                    Some("meow".to_string()),
                    repr::Offset::new(0)
                ))
            )
        ))
    );
    assert_eq!(
        two_opnd(" [X+$3621], [Y-0x3926]"),
        Ok((
            "",
            (
                Operand::AtXn(repr::Offset::new(0x3621)),
                Operand::AtYn(repr::Offset::new(-0x3926))
            )
        ))
    );
    assert_eq!(
        one_opnd(" SP-69"),
        Ok(("", Operand::SPn(repr::Offset::new(-69)))),
    );
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

macro_rules! alts {
    (($(($in:ident, $name:expr)),*)) => {
        alt(($(value(Instruction::$in, tag($name)),)*))
    };
    (Const, ($(($in:ident, $name:expr)),*)) => {
        alt(($(map_res(
                preceded(tag($name), delimited(space1, number_value, space0)),
                |n| Const::new(n).map(Instruction::$in),
        ),)*))
    };
    (Src, ($(($in:ident, $name:expr)),*)) => {
        alt(($(map(
                preceded(tag($name), one_opnd),
                |l| Instruction::$in(Opnd::<Src>::new(l)),
        ),)*))
    };
    (Src, Src, ($(($in:ident, $name:expr)),*)) => {
        alt(($(map(
                preceded(tag($name), two_opnd),
                |(l, r)| Instruction::$in(TwoOpnd::<Src, Src>::new(l, r)),
        ),)*))
    };
    (Dst, ($(($in:ident, $name:expr)),*)) => {
        alt(($(map_res(
                preceded(tag($name), one_opnd),
                |l| Opnd::<Dst>::new(l).map(Instruction::$in),
        ),)*))
    };
    (Dst, $ri:ty, ($(($in:ident, $name:expr)),*)) => {
        alt(($(map_res(
                preceded(tag($name), two_opnd),
                |(l, r)| TwoOpnd::<Dst, $ri>::new(l, r).map(Instruction::$in),
        ),)*))
    };
}

fn instruction(inp: &str) -> IResult<&str, Instruction> {
    // cursed alt nesting courtesy of alt only supporting up to 21
    // items per tuple and rust's opaque types for closures making
    // using an array difficult
    alt((
        alts!(((Nop, "nop"), (Ret, "ret"), (Halt, "halt"))),
        alts!(Const, ((Brk, "brk"), (Sys, "sys"))),
        alts!(Src, ((Jump, "jump"), (Call, "call"), (Push, "push"))),
        alts!(
            Src,
            Src,
            (
                (Bnze, "bnze"),
                (Bze, "bze"),
                (Bpos, "bpos"),
                (Bneg, "bneg"),
                (Out, "out"),
                (Skne, "skne"),
                (Skeq, "skeq"),
                (Sklt, "sklt"),
                (Skgt, "skgt")
            )
        ),
        alts!(
            Dst,
            (
                (Inc, "inc"),
                (Dec, "dec"),
                (Not, "not"),
                (Pop, "pop"),
                (Swap, "swap")
            )
        ),
        alts!(
            Dst,
            Src,
            (
                (Move, "move"),
                (Add, "add"),
                (Sub, "sub"),
                (Mul, "mul"),
                (Div, "div"),
                (And, "and"),
                (Or, "or"),
                (Xor, "xor"),
                (In, "in"),
                (Dbnz, "dbnz"),
                (Mod, "mod"),
                (Shl, "shl"),
                (Shr, "shr"),
                (Addc, "addc"),
                (Mulc, "mulc"),
                (Msb, "msb")
            )
        ),
        alts!(Dst, Dst, ((Xchg, "xchg"))),
        alt((
            map(
                preceded(
                    tag("dw"),
                    preceded(
                        space1,
                        separated_list1(tag(","), delimited(space0, number_words, space0)),
                    ),
                ),
                |v| Instruction::Dw(v.into_iter().flatten().collect()),
            ),
            map(
                preceded(tag("resw"), delimited(space1, number_value, space0)),
                Instruction::Resw,
            ),
        )),
    ))
    .parse(inp)
}

#[test]
fn test_instructions() {
    macro_rules! ins {
        ($case:expr, $($expect:tt)*) => {
            assert_eq!(instruction($case), Ok(("", Instruction::$($expect)*)));
        };
    }
    macro_rules! ins_many {
        ($append:expr, $inner:expr, ($(($case:expr, $arm:ident)),*)) => {
            $(
                ins!(concat!($case, " ", $append), $arm($inner));
            )*
        }
    }

    ins!("nop", Nop);
    ins!("ret", Ret);
    ins!("halt", Halt);
    ins_many!(
        "621",
        Const::new(621).unwrap(),
        (("brk", Brk), ("sys", Sys))
    );
    ins_many!(
        "A",
        Opnd::<Src>::new(Operand::A),
        (("jump", Jump), ("call", Call), ("push", Push))
    );
    ins_many!(
        "A",
        Opnd::<Dst>::new(Operand::A).unwrap(),
        (("inc", Inc), ("dec", Dec), ("not", Not))
    );
    ins_many!(
        "A, B",
        TwoOpnd::<Src, Src>::new(Operand::A, Operand::B),
        (
            ("bnze", Bnze),
            ("bze", Bze),
            ("bpos", Bpos),
            ("bneg", Bneg),
            ("out", Out),
            ("skne", Skne),
            ("skeq", Skeq),
            ("sklt", Sklt),
            ("skgt", Skgt)
        )
    );
    ins_many!(
        "C, D",
        TwoOpnd::<Dst, Src>::new(Operand::C, Operand::D).unwrap(),
        (
            ("move", Move),
            ("add", Add),
            ("sub", Sub),
            ("mul", Mul),
            ("div", Div),
            ("and", And),
            ("or", Or),
            ("xor", Xor),
            ("in", In),
            ("dbnz", Dbnz),
            ("mod", Mod),
            ("shl", Shl),
            ("shr", Shr),
            ("addc", Addc),
            ("mulc", Mulc),
            ("msb", Msb)
        )
    );
    ins!(
        "xchg PC, SP",
        Xchg(TwoOpnd::<Dst, Dst>::new(Operand::PC, Operand::SP).unwrap())
    );
    ins!("dw 1", Dw(vec![1]));
    ins!("dw \"meow\", 0", Dw(vec![109, 101, 111, 119, 0]));
    ins!("dw c\"mow\", 0", Dw(vec![0x6d6f, 0x77, 0]));
    ins!("resw 6", Resw(6));
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

pub fn parse(inp: &str) -> Result<Instructions, Error> {
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
    Ok(Instructions(out))
}

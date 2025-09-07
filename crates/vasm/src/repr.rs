use std::{fmt, marker::PhantomData};

#[derive(Debug, Clone, foxerror::FoxError)]
pub enum Error {
    /// const operands should fit in 10 bits
    BiggerThan10Bits(u16),
    /// destination operands may not use source-exclusive operands
    ///
    /// this includes immediates and relative addresses
    DstSrcExclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Immediate(u16);

impl Immediate {
    // outside of tests, Immediate should be normally constructed with
    // Operand::new_immediate, so that 0 and 1 will use their compact
    // representation
    #[cfg(test)]
    pub fn new(n: u16) -> Self {
        Self(n)
    }
    pub fn value(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for Immediate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryAddress(u16);

impl MemoryAddress {
    pub fn new(n: u16) -> Self {
        Self(n)
    }
    pub fn value(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for MemoryAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offset(i16);

impl Offset {
    pub fn new(n: i16) -> Self {
        Self(n)
    }
    pub fn value(&self) -> i16 {
        self.0
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:+}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelOffset {
    name: Option<String>,
    offset: Offset,
}

impl LabelOffset {
    pub fn new(name: Option<String>, offset: Offset) -> Self {
        Self { name, offset }
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn offset(&self) -> &Offset {
        &self.offset
    }
}

impl fmt::Display for LabelOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{name}")?;
        }
        if self.name.is_none() || self.offset.0 != 0 {
            write!(f, "{}", self.offset)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    /// data register `A`
    A,
    /// data register `B`
    ///
    /// carries end up here
    B,
    /// data register `C`
    C,
    /// data register `D`
    D,
    /// address register `X`
    X,
    /// address register `Y`
    Y,
    /// program counter
    PC,
    /// stack pointer
    SP,
    /// `[X]` value at address `X`
    AtX,
    /// `[Y]` value at address `Y`
    AtY,
    /// `[X++]` value at address `X`, post-increment `X`
    ///
    /// equivalent to the in game assembler's `[X]++`
    AtXInc,
    /// `[Y++]` value at address `Y`, post-increment `Y`
    ///
    /// equivalent to the in game assembler's `[Y]++`
    AtYInc,
    /// `0` a more compact representation of immediate 0
    ///
    /// equivalent to the in game assembler's `#0`
    Immed0,
    /// `1` a more compact representation of immediate 1
    ///
    /// equivalent to the in game assembler's `#1`
    Immed1,
    /// `n` an immediate value
    ///
    /// takes an extra word
    ///
    /// equivalent to the in game assembler's `#n`
    Immediate(Immediate),
    /// `[n]` value at address
    ///
    /// takes an extra word
    ///
    /// equivalent to the in game assembler's `n`
    Mem(MemoryAddress),
    /// `[SP+n]` value at address of stack pointer plus offset
    ///
    /// takes an extra word
    AtSPn(Offset),
    /// `+n`/`-n` relative address or label
    ///
    /// takes an extra word
    Rel2(LabelOffset),
    /// `[X+n]` value at address `X` plus offset
    ///
    /// takes an extra word
    AtXn(Offset),
    /// `[Y+n]` value at address `Y` plus offset
    ///
    /// takes an extra word
    AtYn(Offset),
    /// `SP+n` value at address of stack pointer plus offset
    ///
    /// takes an extra word
    SPn(Offset),
}

impl Operand {
    pub fn new_immediate(n: u16) -> Self {
        match n {
            0 => Self::Immed0,
            1 => Self::Immed1,
            _ => Self::Immediate(Immediate(n)),
        }
    }
    fn is_src_exclusive(&self) -> bool {
        matches!(
            self,
            Self::Immed0 | Self::Immed1 | Self::Immediate(_) | Self::Rel2(_)
        )
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::A => "A",
                Self::B => "B",
                Self::C => "C",
                Self::D => "D",
                Self::X => "X",
                Self::Y => "Y",
                Self::PC => "PC",
                Self::SP => "SP",
                Self::AtX => "[X]",
                Self::AtY => "[Y]",
                Self::AtXInc => "[X++]",
                Self::AtYInc => "[Y++]",
                Self::Immed0 => "0",
                Self::Immed1 => "1",
                Self::Immediate(i) => return write!(f, "{i}"),
                Self::Mem(i) => return write!(f, "[{i}]"),
                Self::AtSPn(i) => return write!(f, "[SP{i}]"),
                Self::Rel2(i) => return write!(f, "{i}"),
                Self::AtXn(i) => return write!(f, "[X{i}]"),
                Self::AtYn(i) => return write!(f, "[Y{i}]"),
                Self::SPn(i) => return write!(f, "SP{i}"),
            }
        )
    }
}

/// the left operand
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opnd1(Operand);

impl Opnd1 {
    pub fn value(&self) -> &Operand {
        &self.0
    }
}

/// the right operand
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opnd2(Operand);

impl Opnd2 {
    pub fn value(&self) -> &Operand {
        &self.0
    }
}

/// generic argument for source operands
///
/// source operands may have immediates
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Src;

/// generic argument for destination operands
///
/// destination operands cannot have immediates
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dst;

/// a single operand
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opnd<Kind> {
    left: Opnd1,
    phantom_kind: PhantomData<Kind>,
}

impl<Kind> Opnd<Kind> {
    pub fn left(&self) -> &Opnd1 {
        &self.left
    }
}

impl Opnd<Src> {
    pub fn new(operand: Operand) -> Self {
        Self {
            left: Opnd1(operand),
            phantom_kind: PhantomData,
        }
    }
}

impl Opnd<Dst> {
    pub fn new(operand: Operand) -> Result<Self, Error> {
        if operand.is_src_exclusive() {
            return Err(Error::DstSrcExclusive);
        }
        Ok(Self {
            left: Opnd1(operand),
            phantom_kind: PhantomData,
        })
    }
}

impl<Kind> fmt::Display for Opnd<Kind> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.left.0)
    }
}

/// two operands
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwoOpnd<LeftKind, RightKind> {
    left: Opnd1,
    right: Opnd2,
    phantom_left: PhantomData<LeftKind>,
    phantom_right: PhantomData<RightKind>,
}

impl<LeftKind, RightKind> TwoOpnd<LeftKind, RightKind> {
    pub fn left(&self) -> &Opnd1 {
        &self.left
    }
    pub fn right(&self) -> &Opnd2 {
        &self.right
    }
}

impl TwoOpnd<Src, Src> {
    pub fn new(left: Operand, right: Operand) -> Self {
        Self {
            left: Opnd1(left),
            right: Opnd2(right),
            phantom_left: PhantomData,
            phantom_right: PhantomData,
        }
    }
}

impl TwoOpnd<Dst, Src> {
    pub fn new(left: Operand, right: Operand) -> Result<Self, Error> {
        if left.is_src_exclusive() {
            return Err(Error::DstSrcExclusive);
        }
        Ok(Self {
            left: Opnd1(left),
            right: Opnd2(right),
            phantom_left: PhantomData,
            phantom_right: PhantomData,
        })
    }
}

impl TwoOpnd<Dst, Dst> {
    pub fn new(left: Operand, right: Operand) -> Result<Self, Error> {
        if left.is_src_exclusive() || right.is_src_exclusive() {
            return Err(Error::DstSrcExclusive);
        }
        Ok(Self {
            left: Opnd1(left),
            right: Opnd2(right),
            phantom_left: PhantomData,
            phantom_right: PhantomData,
        })
    }
}

impl<L, R> fmt::Display for TwoOpnd<L, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.left.0, self.right.0)
    }
}
/// a 10 bit number
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Const(u16);

impl Const {
    pub fn new(n: u16) -> Result<Self, Error> {
        if n >= 1 << 10 {
            return Err(Error::BiggerThan10Bits(n));
        }
        Ok(Self(n))
    }
    pub fn value(&self) -> u16 {
        self.0
    }
}

impl fmt::Display for Const {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    /// no operation
    ///
    /// the name is a lie, it actually yields for around 100ms
    Nop,
    /// breakpoint
    Brk(Const),
    /// syscall
    Sys(Const),
    /// jump to address
    Jump(Opnd<Src>),
    /// call a subroutine
    Call(Opnd<Src>),
    /// return from a subroutine
    Ret,
    /// stop execution
    Halt,
    /// move stuff
    Move(TwoOpnd<Dst, Src>),
    /// swap stuff
    Xchg(TwoOpnd<Dst, Dst>),
    /// increment
    Inc(Opnd<Dst>),
    /// decrement
    Dec(Opnd<Dst>),
    /// addition
    Add(TwoOpnd<Dst, Src>),
    /// subtraction
    Sub(TwoOpnd<Dst, Src>),
    /// multiplication
    Mul(TwoOpnd<Dst, Src>),
    /// division
    Div(TwoOpnd<Dst, Src>),
    /// bitwise and
    And(TwoOpnd<Dst, Src>),
    /// bitwise or
    Or(TwoOpnd<Dst, Src>),
    /// bitwise exclusive or
    Xor(TwoOpnd<Dst, Src>),
    /// bitwise not
    Not(Opnd<Dst>),
    /// branch if not zero
    Bnze(TwoOpnd<Src, Src>),
    /// branch if zero
    Bze(TwoOpnd<Src, Src>),
    /// branch if positive
    Bpos(TwoOpnd<Src, Src>),
    /// branch if negative
    Bneg(TwoOpnd<Src, Src>),
    /// request input from port
    In(TwoOpnd<Dst, Src>),
    /// send output to port
    Out(TwoOpnd<Src, Src>),
    /// push to stack, decrementing stack pointer
    Push(Opnd<Src>),
    /// pop from stack, incrementing stack pointer
    Pop(Opnd<Dst>),
    /// exchange low and high bytes
    Swap(Opnd<Dst>),
    /// decrement and branch if not zero
    Dbnz(TwoOpnd<Dst, Src>),
    /// modulo
    Mod(TwoOpnd<Dst, Src>),
    /// shift left
    Shl(TwoOpnd<Dst, Src>),
    /// shift right
    Shr(TwoOpnd<Dst, Src>),
    /// add with carry
    ///
    /// carry ends up in register B
    Addc(TwoOpnd<Dst, Src>),
    /// multiply with carry
    ///
    /// carry ends up in register B
    Mulc(TwoOpnd<Dst, Src>),
    /// skip next *two words* if not equal
    Skne(TwoOpnd<Src, Src>),
    /// ship next *two words* if equal
    Skeq(TwoOpnd<Src, Src>),
    /// ship next *two words* if less than
    Sklt(TwoOpnd<Src, Src>),
    /// ship next *two words* if greater than
    Skgt(TwoOpnd<Src, Src>),
    /// get most significant bit
    Msb(TwoOpnd<Dst, Src>),
    /// global label definition
    ///
    /// not a real opcode, will not show up in the assembled output
    LabelDef(String),
    /// a comment
    ///
    /// not a real opcode, will not show up in the assembled output
    Comment(String),
    /// define word
    ///
    /// not a real opcode, will output the data untouched
    ///
    /// this has some syntax sugar for including both packed and single
    /// character per word strings
    /// ```custom
    /// dw c"mow", 0, "mow", 0
    /// ```
    /// which is equivalent to
    /// ```custom
    /// dw 0x6d6f, 0x0077, 0x0000, 0x006d, 0x006f, 0x0077, 0x0000
    /// ```
    Dw(Vec<u16>),
    /// reserve a number of words without specifying the contents
    ///
    /// not a real opcode, may output zeros or leave what was there
    /// previously untouched
    Resw(u16),
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        macro_rules! w {
            ($fmt:expr, $($arg:tt)*) => {
                write!(f, concat!("\t", $fmt), $($arg)*)
            };
            ($fmt:expr) => {
                w!($fmt,)
            };
        }

        match self {
            Self::Nop => w!("nop"),
            Self::Brk(o) => w!("brk {}", o),
            Self::Sys(o) => w!("sys {}", o),
            Self::Jump(o) => w!("jump {}", o),
            Self::Call(o) => w!("call {}", o),
            Self::Ret => w!("ret"),
            Self::Halt => w!("halt"),
            Self::Move(o) => w!("move {}", o),
            Self::Xchg(o) => w!("xchg {}", o),
            Self::Inc(o) => w!("inc {}", o),
            Self::Dec(o) => w!("dec {}", o),
            Self::Add(o) => w!("add {}", o),
            Self::Sub(o) => w!("sub {}", o),
            Self::Mul(o) => w!("mul {}", o),
            Self::Div(o) => w!("div {}", o),
            Self::And(o) => w!("and {}", o),
            Self::Or(o) => w!("or {}", o),
            Self::Xor(o) => w!("xor {}", o),
            Self::Not(o) => w!("not {}", o),
            Self::Bnze(o) => w!("bnze {}", o),
            Self::Bze(o) => w!("bze {}", o),
            Self::Bpos(o) => w!("bpos {}", o),
            Self::Bneg(o) => w!("bneg {}", o),
            Self::In(o) => w!("in {}", o),
            Self::Out(o) => w!("out {}", o),
            Self::Push(o) => w!("push {}", o),
            Self::Pop(o) => w!("pop {}", o),
            Self::Swap(o) => w!("swap {}", o),
            Self::Dbnz(o) => w!("swap {}", o),
            Self::Mod(o) => w!("mod {}", o),
            Self::Shl(o) => w!("shl {}", o),
            Self::Shr(o) => w!("shr {}", o),
            Self::Mulc(o) => w!("mulc {}", o),
            Self::Skne(o) => w!("skne {}", o),
            Self::Skeq(o) => w!("skeq {}", o),
            Self::Sklt(o) => w!("sklt {}", o),
            Self::Skgt(o) => w!("skgt {}", o),
            Self::Addc(o) => w!("addc {}", o),
            Self::Msb(o) => w!("msb {}", o),
            Self::LabelDef(n) => write!(f, "{n}:"),
            Self::Comment(o) => w!(";{}", o),
            Self::Dw(v) => {
                write!(f, "\tdw")?;
                let mut sep = " ";
                for i in v {
                    write!(f, "{sep}{i:#x}")?;
                    sep = ", ";
                }
                Ok(())
            }
            Self::Resw(o) => w!("resw {}", o),
        }
    }
}

pub struct Instructions(pub Vec<Instruction>);

impl fmt::Display for Instructions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in &self.0 {
            writeln!(f, "{i}")?;
        }
        Ok(())
    }
}

use std::marker::PhantomData;

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
    pub fn value(&self) -> u16 {
        self.0
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
    AtXInc,
    /// `[Y++]` value at address `Y`, post-increment `Y`
    AtYInc,
    /// `0` a more compact representation of immediate 0
    Immed0,
    /// `1` a more compact representation of immediate 1
    Immed1,
    /// `n` an immediate value
    ///
    /// takes an extra word
    Immediate(Immediate),
    /// `[n]` value at address
    ///
    /// takes an extra word
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
    #[allow(dead_code)]
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
    /// and for including a number of zeros
    /// ```custom
    /// resw 6
    /// ```
    /// which is equivalent to
    /// ```custom
    /// dw 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000
    /// ```
    Dw(Vec<u16>),
}

// TODO: pretty print instead of just using debug
impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

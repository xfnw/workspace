#![allow(dead_code)]

use std::marker::PhantomData;

pub struct Immediate(i16);

pub struct MemoryAddress(u16);

pub struct Offset(i16);

pub struct LabelOffset {
    name: Option<String>,
    offset: Offset,
}

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
    /// `[X]+` value at address `X`, post-increment `X`
    AtXInc,
    /// `[Y]+` value at address `Y`, post-increment `Y`
    AtYInc,
    /// `#0` a more compact representation of immediate 0
    Const0,
    /// `#1` a more compact representation of immediate 1
    Const1,
    /// `#n` an immediate value
    ///
    /// takes an extra word
    Immediate(Immediate),
    /// `n` value at address
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

/// the left operand
pub struct Opnd1(Operand);

/// the right operand
pub struct Opnd2(Operand);

/// generic argument for source operands
///
/// source operands may have immediates
pub struct Src;

/// generic argument for destination operands
///
/// destination operands cannot have immediates
pub struct Dst;

/// a single operand
pub struct Opnd<Kind> {
    left: Opnd1,
    phantom_kind: PhantomData<Kind>,
}

/// two operands
pub struct TwoOpnd<LeftKind, RightKind> {
    left: Opnd1,
    right: Opnd2,
    phantom_left: PhantomData<LeftKind>,
    phantom_right: PhantomData<RightKind>,
}

/// a 10 bit number
pub struct Const(u16);

pub enum Opcode {
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
}

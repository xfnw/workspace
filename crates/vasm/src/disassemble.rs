use crate::repr::{Instruction, Instructions};

pub fn disassemble(bytes: &[u16]) -> Instructions {
    Instructions(vec![Instruction::Dw(bytes.to_vec())])
}

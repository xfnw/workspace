use super::repr;

#[derive(Debug, foxerror::FoxError)]
pub enum Error {
    /// could not build internal representation
    #[err(from)]
    Repr(repr::Error),
}

pub fn parse(inp: &str) -> Result<Vec<repr::Opcode>, Error> {
    todo!()
}

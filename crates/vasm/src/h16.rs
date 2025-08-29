use std::fmt;

pub struct H16Display<'a> {
    slice: &'a [u16],
    start: u16,
}

impl<'a> H16Display<'a> {
    pub const fn new(start: u16, slice: &'a [u16]) -> Self {
        Self { slice, start }
    }
}

#[allow(clippy::cast_possible_truncation)]
impl fmt::Display for H16Display<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (n, chunk) in self.slice.chunks(8).enumerate() {
            write!(
                f,
                ":{:01X}{:04X}00",
                chunk.len(),
                self.start.wrapping_add((n * 8) as u16)
            )?;

            for w in chunk {
                write!(f, "{w:04X}")?;
            }

            writeln!(f)?;
        }

        writeln!(f, ":00000FF")
    }
}

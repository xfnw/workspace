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

#[derive(Debug, foxerror::FoxError)]
pub enum Error {
    /// unexpected EOF
    Truncated,
    /// expected colon
    ExpectedColon,
    /// a single line may only contain 8 words or less
    InvalidLineLen,
    /// could not parse number
    ParseU16,
    /// only 00 and FF line types are supported
    UnknownLineKind,
}

// TODO: replace with from_ascii_radix once stabilized
fn from_ascii_hex(b: &[u8]) -> Result<u16, Error> {
    let s = str::from_utf8(b).map_err(|_| Error::ParseU16)?;
    u16::from_str_radix(s, 16).map_err(|_| Error::ParseU16)
}

pub fn parse<T>(mut inp: T, start: u16) -> Result<Vec<u16>, Error>
where
    T: Iterator<Item = u8>,
{
    let mut out = vec![];

    loop {
        match inp.next().ok_or(Error::Truncated)? {
            b':' => (),
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => return Err(Error::ExpectedColon),
        }
        let num_words = inp.next().ok_or(Error::Truncated)?.wrapping_sub(0x30);
        if num_words > 8 {
            return Err(Error::InvalidLineLen);
        }
        let addr = from_ascii_hex(&inp.by_ref().take(4).collect::<Vec<_>>())?;
        let kind: Vec<_> = inp.by_ref().take(2).collect();
        match (num_words, addr, kind.as_slice()) {
            (_, _, b"00") => (),
            (0, 0, b"FF") => break,
            _ => return Err(Error::UnknownLineKind),
        }
        let addr = addr.wrapping_sub(start);
        match addr.checked_add(num_words.into()) {
            Some(end) => {
                if end as usize > out.len() {
                    out.resize(end.into(), 0);
                } else {
                    eprintln!("possibly overwriting stuff!");
                }
            }
            None => out.resize(u16::MAX as usize + 1, 0),
        }
        for n in 0..num_words {
            let val = from_ascii_hex(&inp.by_ref().take(4).collect::<Vec<_>>())?;
            out[addr.wrapping_add(n.into()) as usize] = val;
        }
    }

    Ok(out)
}

#[test]
fn unh16() {
    let h = ":8E6210000010002000300040005000600070008
:2E629000009000A
:00000FF";
    let parsed = parse(h.bytes(), 0xe621).unwrap();
    assert_eq!(parsed, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

    let h = ":2FFFF0000010002
:00000FF";
    let parsed = parse(h.bytes(), 0).unwrap();
    assert_eq!(parsed[u16::MAX as usize], 1);
    assert_eq!(parsed[0], 2);
}

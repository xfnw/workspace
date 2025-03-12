use lazy_regex::regex_captures;
use std::{
    convert::From,
    fmt,
    fs::File,
    io::{Read, Seek, Write},
    path::PathBuf,
    process::exit,
};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// check if formatted
    #[arg(short)]
    check: bool,
    /// overwrite with formatted
    #[arg(short)]
    fix: bool,
    #[arg(default_value = "/dev/stdin")]
    files: Vec<PathBuf>,
}

#[derive(Debug)]
struct InfoLine {
    key: Option<String>,
    value: Option<String>,
    indent: usize,
}

impl InfoLine {
    fn parse_line(inp: &str, prev: isize) -> (Self, isize) {
        let (_, key, whitespace, value) =
            regex_captures!(r"^(?:([^ \t]*):)?([ \t]*)((?:[^ \t].*)?)$", inp).unwrap();
        let (new, indent) = if key.is_empty() {
            (prev, whitespace.len() as isize - prev)
        } else {
            ((key.len() + whitespace.len() + 1) as isize, 0)
        };
        let out = Self {
            key: if key.is_empty() {
                None
            } else {
                Some(key.to_string())
            },
            value: if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            },
            indent: indent.try_into().unwrap_or(0),
        };
        (out, new)
    }
}

struct InfoFile {
    lines: Vec<InfoLine>,
}

impl InfoFile {
    fn margin(&self) -> Option<usize> {
        self.lines
            .iter()
            .filter_map(|l| l.key.as_ref())
            .map(|k| k.len() + 2)
            .max()
    }
}

impl<T: AsRef<str>> From<T> for InfoFile {
    fn from(inp: T) -> Self {
        let mut wlen = isize::MAX;
        let mut lines = Vec::new();
        for line in inp.as_ref().lines() {
            let (parsed, new) = InfoLine::parse_line(line, wlen);
            lines.push(parsed);
            wlen = new;
        }
        Self { lines }
    }
}

impl fmt::Display for InfoFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use std::fmt::Write;

        let margin = self.margin().unwrap_or(0);
        for line in &self.lines {
            let mut written = 0;

            if let Some(k) = &line.key {
                f.write_str(k)?;
                f.write_char(':')?;
                written += k.len() + 1;
            }

            if let Some(v) = &line.value {
                while written < margin + line.indent {
                    f.write_char(' ')?;
                    written += 1;
                }
                f.write_str(v)?;
            }

            f.write_char('\n')?;
        }
        Ok(())
    }
}

pub fn run(args: &Args) {
    let mut ret = 0;
    for name in &args.files {
        let mut file = File::options()
            .read(true)
            .write(args.fix)
            .open(name)
            .unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let parsed = InfoFile::from(&contents);
        if args.check {
            if contents != parsed.to_string() {
                eprintln!("{name:?} differs");
                ret = 1;
            }
        } else if args.fix {
            file.rewind().unwrap();
            write!(file, "{parsed}").unwrap();
        } else {
            print!("{parsed}");
        }
    }
    exit(ret);
}

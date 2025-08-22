use std::{
    fs::{read, read_to_string},
    io::{Write, stdout},
    path::PathBuf,
};

/// uwu owo uwu owo
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "uwu")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    /// the action to do (encode or decode)
    #[argh(positional)]
    action: Action,
    #[argh(positional, greedy)]
    files: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
enum Action {
    Encode,
    Decode,
}

impl std::str::FromStr for Action {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "encode" | "e" => Ok(Self::Encode),
            "decode" | "d" => Ok(Self::Decode),
            _ => Err("action should be encode or decode"),
        }
    }
}

static UWUS: [&str; 16] = [
    "uwu", "owo", "umu", "nya", "omo", "o_o", "q_p", "u_u", "o~o", "UwU", "OwO", "UmU", "OmO",
    "O_O", "U_U", "Nya",
];

fn uwu_byte(byte: u8) -> [&'static str; 2] {
    let left = byte >> 4;
    let right = byte & 15;
    [UWUS[left as usize], UWUS[right as usize]]
}

fn unuwu_nibble(word: &str) -> Option<u8> {
    #[allow(clippy::cast_possible_truncation)]
    UWUS.iter().position(|i| *i == word).map(|n| n as u8)
}

fn unuwu_string(s: &str) -> Vec<u8> {
    s.split_ascii_whitespace()
        .filter_map(unuwu_nibble)
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|c| (c[0] << 4) + c.get(1).unwrap_or(&0))
        .collect()
}

pub fn run(args: &Args) {
    let files = if args.files.is_empty() {
        &vec![PathBuf::from("/dev/stdin")]
    } else {
        &args.files
    };
    match &args.action {
        Action::Encode => {
            for name in files {
                println!(
                    "{}",
                    read(name)
                        .unwrap()
                        .into_iter()
                        .flat_map(uwu_byte)
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
        Action::Decode => {
            for name in files {
                let s = read_to_string(name).unwrap();
                stdout().write_all(&unuwu_string(&s)).unwrap();
            }
        }
    }
}

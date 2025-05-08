use std::{
    fs::{read, read_to_string},
    io::{Write, stdout},
    path::PathBuf,
};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Clone, Debug, clap::Subcommand)]
enum Action {
    Encode {
        #[arg(default_value = "/dev/stdin")]
        files: Vec<PathBuf>,
    },
    Decode {
        #[arg(default_value = "/dev/stdin")]
        files: Vec<PathBuf>,
    },
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
    match &args.action {
        Action::Encode { files } => {
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
        Action::Decode { files } => {
            for name in files {
                let s = read_to_string(name).unwrap();
                stdout().write_all(&unuwu_string(&s)).unwrap();
            }
        }
    }
}

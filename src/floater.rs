use half::{bf16, f16};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[arg(value_enum)]
    size: Size,
    number: f64,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum Size {
    F64,
    F32,
    F16,
    BF16,
}

pub fn run(args: &Args) {
    println!(
        "{}",
        match args.size {
            Size::F64 => {
                let float = args.number;
                let new = f64::from_bits(float.to_bits() + 1);
                new - float
            }
            Size::F32 => {
                #[allow(clippy::cast_possible_truncation)]
                let float = args.number as f32;
                let new = f32::from_bits(float.to_bits() + 1);
                (new - float).into()
            }
            Size::F16 => {
                let float = f16::from_f64(args.number);
                let new = f16::from_bits(float.to_bits() + 1);
                (new - float).into()
            }
            Size::BF16 => {
                let float = bf16::from_f64(args.number);
                let new = bf16::from_bits(float.to_bits() + 1);
                (new - float).into()
            }
        }
    );
}

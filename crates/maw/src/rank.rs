// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

/// rank stuff using elo
#[derive(Debug, argh::FromArgs)]
#[argh(subcommand, name = "rank")]
#[argh(help_triggers("-h", "--help"))]
pub struct Args {
    /// maximum possible adjustment K-factor (defaults to 32)
    #[argh(option, short = 'k', default = "32.0")]
    max_adjustment: f64,
    /// initial rating for everyone (defaults to 1500)
    #[argh(option, short = 'i', default = "1500.0")]
    initial: f64,
    #[argh(positional, greedy)]
    files: Vec<PathBuf>,
}

// https://en.wikipedia.org/wiki/Elo_rating_system#Mathematical_details
fn predict(a: f64, b: f64) -> (f64, f64) {
    let q_a = 10f64.powf(a / 400.0);
    let q_b = 10f64.powf(b / 400.0);
    let expected_a = q_a / (q_a + q_b);
    let expected_b = q_b / (q_a + q_b);
    (expected_a, expected_b)
}

fn new_ratings(winner: f64, loser: f64, max_adjustment: f64) -> (f64, f64) {
    let (expected_win, expected_lose) = predict(winner, loser);
    let new_win = winner + max_adjustment * (1.0 - expected_win);
    let new_lose = loser + max_adjustment * (0.0 - expected_lose);

    (new_win, new_lose)
}

pub fn run(args: &Args) {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let files = if args.files.is_empty() {
        &vec![PathBuf::from("/dev/stdin")]
    } else {
        &args.files
    };

    for name in files {
        for line in BufReader::new(File::open(name).unwrap()).lines() {
            let line = line.unwrap();
            let split: Vec<_> = line.split('\t').collect();

            match split[..] {
                [winner, loser] => {
                    let old_win = scores.get(winner).copied().unwrap_or(args.initial);
                    let old_lose = scores.get(loser).copied().unwrap_or(args.initial);
                    let (new_win, new_lose) = new_ratings(old_win, old_lose, args.max_adjustment);
                    scores.insert(winner.to_string(), new_win);
                    scores.insert(loser.to_string(), new_lose);
                }
                [id] => println!("{}\t{id}", scores.get(id).copied().unwrap_or(args.initial)),
                ["set", fst, snd] if let Ok(rating) = snd.parse() => {
                    scores.insert(fst.to_string(), rating);
                }
                _ => eprintln!("skipping malformed line: {line:?}"),
            }
        }
    }

    let mut output: Vec<_> = scores.drain().map(|(id, score)| (score, id)).collect();
    output.sort_unstable_by(|(score_a, id_a), (score_b, id_b)| {
        score_a.total_cmp(score_b).then_with(|| id_a.cmp(id_b))
    });
    for (score, id) in output {
        println!("{score:.2}\t{id}");
    }
}

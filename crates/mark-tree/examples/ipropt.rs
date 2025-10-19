// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MIT

use mark_tree::{IpRange, MarkTree};

fn main() {
    let mut ipees = MarkTree::new();
    for line in std::io::stdin().lines() {
        let line = line.unwrap();
        let Ok(range) = line.parse::<IpRange>() else {
            eprintln!("ignoring {line:?}");
            continue;
        };
        ipees.mark(range.iter());
    }

    eprintln!("optimizing...");
    ipees.optimize();

    ipees.traverse(|tree, bits| {
        if !matches!(tree, MarkTree::AllMarked) {
            return;
        }
        let range = IpRange::from_bits(bits).unwrap();
        println!("{range}");
    });
}

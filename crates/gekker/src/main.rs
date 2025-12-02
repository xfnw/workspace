// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeSet, hash::Hasher};
use tokio::sync::{RwLock, mpsc};

#[derive(Debug)]
struct Client {
    nick: String,
    sender: mpsc::Sender<Vec<u8>>,
}

#[derive(Debug)]
struct State {
    clients: RwLock<Vec<Option<Client>>>,
    active: RwLock<BTreeSet<usize>>,
    callback: RwLock<mpsc::Sender<u64>>,
}

fn hash_line(nick: &[u8], command: &[u8], trail: &[u8]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    hasher.write(nick);
    hasher.write(b" ");
    hasher.write(command);
    hasher.write(b" ");
    hasher.write(trail);
    hasher.finish()
}

fn main() {
    println!("Hello, world!");
}

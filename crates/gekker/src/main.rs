// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeSet;
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

fn main() {
    println!("Hello, world!");
}

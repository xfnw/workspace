// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use argh::{FromArgs, from_env};

/// a more helpful vet
#[derive(Debug, FromArgs)]
struct Opt {}

fn main() {
    let _opt: Opt = from_env();
}

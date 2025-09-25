// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::Error;
use rayon::prelude::*;
use std::process::ExitCode;

pub fn do_check(args: &crate::CheckArgs) -> Result<ExitCode, Error> {
    let dependencies = crate::metadata::get_dependencies(&args.lock)?;
    if dependencies.is_empty() {
        return Err(Error::EmptyDependencies);
    }

    let unaudited: Vec<_> = dependencies
        .into_par_iter()
        .flat_map(|(_name, _version)| Some(1))
        .collect();

    if unaudited.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    for c in unaudited {
        println!("oh no {c}");
    }

    Ok(ExitCode::FAILURE)
}

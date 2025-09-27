// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::Error;
use std::{
    io::{Seek, Write},
    process::ExitCode,
};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

pub fn add_audit(args: &crate::AuditArgs) -> Result<ExitCode, Error> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&args.audits)
        .map_err(Error::AuditsOpen)?;
    let mut toml: DocumentMut = std::io::read_to_string(&file)
        .map_err(Error::AuditsOpen)?
        .parse()?;

    let Item::Table(atable) = toml
        .entry("audits")
        .or_insert_with(|| Item::Table(Table::new()))
    else {
        return Err(Error::TomlBorked);
    };
    if atable.is_empty() {
        atable.set_implicit(true);
    }

    let Item::ArrayOfTables(arr) = atable
        .entry(&args.name)
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
    else {
        return Err(Error::TomlBorked);
    };

    let mut t = Table::new();
    if let Some(base) = &args.base {
        t["delta"] = value(format!("{base} -> {}", args.version));
    } else {
        t["version"] = value(&args.version);
    }
    t["criteria"] = value(&args.criteria);
    if let Some(notes) = &args.notes {
        t["notes"] = value(notes);
    }
    arr.push(t);

    file.rewind().map_err(Error::AuditsWrite)?;
    file.set_len(0).map_err(Error::AuditsWrite)?;
    file.write_all(toml.to_string().as_bytes())
        .map_err(Error::AuditsWrite)?;

    eprintln!("added :3");
    Ok(ExitCode::SUCCESS)
}

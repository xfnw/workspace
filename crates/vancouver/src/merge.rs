// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::Error;
use std::{
    collections::BTreeSet,
    io::{Seek, Write},
    process::ExitCode,
};
use toml_edit::{ArrayOfTables, DocumentMut, Formatted, Item, Table, Value, value};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DupeKey {
    name: String,
    criteria: String,
    delta: Option<String>,
    version: Option<String>,
    violation: Option<String>,
}

impl DupeKey {
    fn new(name: &str, audit: &Table) -> Option<Self> {
        let Some(Item::Value(Value::String(criteria))) = audit.get("criteria") else {
            return None;
        };
        let delta = if let Some(Item::Value(Value::String(s))) = audit.get("delta") {
            Some(s.value().clone())
        } else {
            None
        };
        let version = if let Some(Item::Value(Value::String(s))) = audit.get("version") {
            Some(s.value().clone())
        } else {
            None
        };
        let violation = if let Some(Item::Value(Value::String(s))) = audit.get("violation") {
            Some(s.value().clone())
        } else {
            None
        };
        Some(Self {
            name: name.to_string(),
            criteria: criteria.value().clone(),
            delta,
            version,
            violation,
        })
    }
}

pub fn do_merge(args: &crate::MergeArgs) -> Result<ExitCode, Error> {
    let source = std::fs::read_to_string(&args.file).map_err(Error::MergeSourceOpen)?;
    let source: DocumentMut = source.parse()?;
    let Some(Item::Table(source_audits_table)) = source.get("audits") else {
        return Err(Error::TomlBorked);
    };

    let mut destfile = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&args.audits)
        .map_err(Error::AuditsOpen)?;
    let mut dest: DocumentMut = std::io::read_to_string(&destfile)
        .map_err(Error::AuditsOpen)?
        .parse()?;
    let Item::Table(dest_audits_table) = dest
        .entry("audits")
        .or_insert_with(|| Item::Table(Table::new()))
    else {
        return Err(Error::TomlBorked);
    };
    if dest_audits_table.is_empty() {
        dest_audits_table.set_implicit(true);
    }

    let existing: BTreeSet<_> = dest_audits_table
        .iter()
        .filter_map(|(key, inner)| {
            let Item::ArrayOfTables(inner) = inner else {
                return None;
            };
            Some(
                inner
                    .iter()
                    .filter_map(|t| DupeKey::new(key, t))
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect();

    let mut merged = BTreeSet::new();
    for (dep, inner) in source_audits_table {
        let Item::ArrayOfTables(inner) = inner else {
            continue;
        };
        let Item::ArrayOfTables(darr) = dest_audits_table
            .entry(dep)
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        else {
            return Err(Error::TomlBorked);
        };
        for audit in inner {
            if let Some(Item::Value(Value::Boolean(b))) = audit.get("private")
                && *b.value()
            {
                continue;
            }
            let mut t = audit.clone();
            if args.isolate
                && let Some(Item::Value(Value::String(s))) = t.get_mut("criteria")
            {
                // FIXME: this eats comments on the criteria key :/
                *s = Formatted::new(format!("{}:{}", args.identifier, s.value()));
            }
            let Some(dup) = DupeKey::new(dep, &t) else {
                continue;
            };
            let exists = existing.contains(&dup);
            merged.insert(dup);
            if exists {
                continue;
            }
            t["merged-from"] = value(args.identifier.clone());
            if args.isolate {
                t["private"] = value(true);
            }
            darr.push(t);
        }
    }

    dest_audits_table.retain(|name, inner| {
        let Item::ArrayOfTables(inner) = inner else {
            return true;
        };

        inner.retain(|t| {
            let Some(Item::Value(Value::String(from))) = t.get("merged-from") else {
                return true;
            };
            let Some(dup) = DupeKey::new(name, t) else {
                return true;
            };
            from.value() != &args.identifier || merged.contains(&dup)
        });

        !inner.is_empty()
    });

    if dest_audits_table.is_empty() {
        dest_audits_table.set_implicit(false);
    }
    destfile.rewind().map_err(Error::AuditsWrite)?;
    destfile.set_len(0).map_err(Error::AuditsWrite)?;
    destfile
        .write_all(dest.to_string().as_bytes())
        .map_err(Error::AuditsWrite)?;

    eprintln!("merged :3");
    Ok(ExitCode::SUCCESS)
}

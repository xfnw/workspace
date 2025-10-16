// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::{
    OutputFormat,
    de::string_or_bset,
    types::{Error, Version},
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::read_to_string,
    io::{Seek, Write},
    process::ExitCode,
    sync::atomic::{AtomicBool, Ordering},
};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value, value};

#[derive(Debug, Deserialize)]
struct Config {
    /// extra criteria that overrides criteria specified in the audits
    /// file
    #[serde(default)]
    criteria: BTreeMap<String, Criteria>,
    /// policy for crates without a policy specified
    #[serde(default, alias = "default-policy")]
    default_policy: PolicyLayer,
    #[serde(default)]
    policy: BTreeMap<String, PolicyLayer>,
    #[serde(default)]
    exempt: BTreeMap<String, BTreeSet<Audit>>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct PolicyLayer {
    #[serde(default, alias = "require-all")]
    require_all: Option<BTreeSet<String>>,
}

#[derive(Debug, Clone)]
struct Policy {
    require_all: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
struct Audits {
    /// criteria that can be overridden by the config file
    #[serde(default)]
    criteria: BTreeMap<String, Criteria>,
    audits: BTreeMap<String, BTreeSet<Audit>>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct Audit {
    /// the criteria audited for
    criteria: String,
    /// the version range for an audit of the changes between two
    /// versions
    #[serde(default)]
    delta: Option<String>,
    /// the version for a standalone audit of an entire version
    #[serde(default)]
    version: Option<String>,
    /// the version for a standalone violation of an entire version
    ///
    /// unlike cargo-vet, this must be a single specific version,
    /// rather than a version range
    #[serde(default)]
    violation: Option<String>,
    /// do not warn when this is an unused exemption
    ///
    /// this is only meaningful when specified on exemptions in the
    /// config, unused audits do not cause warnings
    #[serde(default, alias = "allow-unused")]
    allow_unused: bool,
}

#[derive(Debug, Deserialize, Clone)]
struct Criteria {
    /// give the listed criteria to everything that has this criteria
    #[serde(default, deserialize_with = "string_or_bset")]
    implies: BTreeSet<String>,
    /// automatically imply this criteria on everything that has all
    /// of the listed criteria
    #[serde(default, deserialize_with = "string_or_bset", alias = "implied-all")]
    implied_all: BTreeSet<String>,
    /// automatically imply this criteria on everything that has any
    /// of the listed criteria
    #[serde(default, deserialize_with = "string_or_bset", alias = "implied-any")]
    implied_any: BTreeSet<String>,
}

#[derive(Debug)]
struct UsedMarker(Option<AtomicBool>);

impl UsedMarker {
    fn mark_used(&self) {
        if let Some(b) = &self.0 {
            b.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Debug)]
struct TrustRoot {
    used: UsedMarker,
}

#[derive(Debug)]
struct TrustDelta {
    parent_version: Version,
}

#[derive(Debug)]
struct CriteriaCons<'a>(&'a str, Option<&'a CriteriaCons<'a>>);

impl<'a> CriteriaCons<'a> {
    fn iter(&'a self) -> CriteriaConsIter<'a> {
        CriteriaConsIter(Some(self))
    }
}

struct CriteriaConsIter<'a>(Option<&'a CriteriaCons<'a>>);

impl<'a> Iterator for CriteriaConsIter<'a> {
    type Item = &'a str;

    #[allow(clippy::similar_names)]
    fn next(&mut self) -> Option<Self::Item> {
        let CriteriaCons(car, cdr) = self.0?;

        self.0 = *cdr;
        Some(car)
    }
}

type CriteriaMap<T> = BTreeMap<String, T>;
type DepMap<T> = BTreeMap<String, T>;

#[derive(Debug)]
struct Rules {
    trust_roots: CriteriaMap<DepMap<BTreeMap<Version, TrustRoot>>>,
    trust_deltas: CriteriaMap<DepMap<BTreeMap<Version, TrustDelta>>>,
    violations: CriteriaMap<DepMap<BTreeSet<Version>>>,
    extra_unused: BTreeSet<UnusedExempt>,
    implied_all: CriteriaMap<BTreeSet<String>>,
    implied_any: CriteriaMap<BTreeSet<String>>,
    default_policy: Policy,
    policy: DepMap<Policy>,
}

impl Rules {
    #[allow(clippy::too_many_lines)]
    fn new(mut config: Config, audits: Audits) -> Result<Self, Error> {
        let mut criteria = audits.criteria;
        criteria.append(&mut config.criteria);

        let mut implied_all = BTreeMap::new();
        let mut implied_any: CriteriaMap<BTreeSet<String>> = BTreeMap::new();

        for (
            criteria,
            Criteria {
                implies: imp,
                implied_all: all,
                implied_any: mut any,
            },
        ) in criteria
        {
            for i in imp {
                implied_any.entry(i).or_default().insert(criteria.clone());
            }
            implied_all.insert(criteria.clone(), all);
            implied_any.entry(criteria).or_default().append(&mut any);
        }

        let mut trust_roots: CriteriaMap<DepMap<BTreeMap<Version, TrustRoot>>> = BTreeMap::new();
        let mut trust_deltas: CriteriaMap<DepMap<BTreeMap<Version, TrustDelta>>> = BTreeMap::new();
        let mut violations: CriteriaMap<DepMap<BTreeSet<Version>>> = BTreeMap::new();

        // TODO: squish this into a macro
        for (name, aset) in config.exempt {
            for Audit {
                criteria,
                version,
                allow_unused,
                ..
            } in aset
            {
                if let Some(version) = &version {
                    trust_roots
                        .entry(criteria.clone())
                        .or_default()
                        .entry(name.clone())
                        .or_default()
                        .insert(
                            Version::new(version),
                            TrustRoot {
                                used: UsedMarker(Some(allow_unused.into())),
                            },
                        );
                }
            }
        }

        let mut extra_unused = BTreeSet::new();

        for (name, aset) in audits.audits {
            for Audit {
                criteria,
                delta,
                version,
                violation,
                ..
            } in aset
            {
                if let Some(delta) = &delta {
                    let (prev, next) = parse_delta(delta)?;
                    trust_deltas
                        .entry(criteria.clone())
                        .or_default()
                        .entry(name.clone())
                        .or_default()
                        .insert(
                            next,
                            TrustDelta {
                                parent_version: prev,
                            },
                        );
                }
                if let Some(version) = &version
                    && trust_roots
                        .entry(criteria.clone())
                        .or_default()
                        .entry(name.clone())
                        .or_default()
                        .insert(
                            Version::new(version),
                            TrustRoot {
                                used: UsedMarker(None),
                            },
                        )
                        .is_some_and(|r| r.used.0.is_some())
                {
                    extra_unused.insert(UnusedExempt {
                        name: name.clone(),
                        version: version.clone(),
                        criteria: criteria.clone(),
                    });
                }
                if let Some(violation) = &violation {
                    violations
                        .entry(criteria)
                        .or_default()
                        .entry(name.clone())
                        .or_default()
                        .insert(Version::new(violation));
                }
            }
        }

        let default_policy = Policy {
            require_all: config
                .default_policy
                .require_all
                .unwrap_or_else(|| ["safe-to-deploy".to_string()].into()),
        };
        let policy = config
            .policy
            .into_iter()
            .map(|(d, p)| {
                (
                    d,
                    Policy {
                        require_all: p
                            .require_all
                            .unwrap_or_else(|| default_policy.require_all.clone()),
                    },
                )
            })
            .collect();

        Ok(Self {
            trust_roots,
            trust_deltas,
            violations,
            extra_unused,
            implied_all,
            implied_any,
            default_policy,
            policy,
        })
    }

    fn get_policy(&self, name: &str) -> &Policy {
        self.policy.get(name).unwrap_or(&self.default_policy)
    }

    fn check_criteria(
        &self,
        name: &str,
        version: &Version,
        criteria: &str,
        implied_criteria: Option<&CriteriaCons>,
        recursion_limit: usize,
    ) -> CheckResult {
        if self
            .violations
            .get(criteria)
            .and_then(|d| d.get(name))
            .is_some_and(|v| v.contains(version))
        {
            return CheckResult::Violation;
        }

        if let Some(trust) = self
            .trust_roots
            .get(criteria)
            .and_then(|d| d.get(name))
            .and_then(|v| v.get(version))
            .or_else(|| {
                implied_criteria
                    .iter()
                    .flat_map(|c| c.iter())
                    .find_map(|cr| {
                        self.trust_roots
                            .get(cr)
                            .and_then(|d| d.get(name))
                            .and_then(|v| v.get(version))
                    })
            })
        {
            trust.used.mark_used();
            return CheckResult::Validated;
        }

        if recursion_limit == 0 {
            return CheckResult::RecursionLimitReached;
        }

        if let Some(trust) = self
            .trust_deltas
            .get(criteria)
            .and_then(|d| d.get(name))
            .and_then(|v| v.get(version))
            .or_else(|| {
                implied_criteria
                    .iter()
                    .flat_map(|c| c.iter())
                    .find_map(|cr| {
                        self.trust_deltas
                            .get(cr)
                            .and_then(|d| d.get(name))
                            .and_then(|v| v.get(version))
                    })
            })
        {
            return self.check_criteria(
                name,
                &trust.parent_version,
                criteria,
                implied_criteria,
                recursion_limit - 1,
            );
        }

        if let Some(cr) = self.implied_all.get(criteria)
            && !cr.is_empty()
            && cr.iter().all(|c| {
                self.check_criteria(
                    name,
                    version,
                    c,
                    Some(&CriteriaCons(criteria, implied_criteria)),
                    recursion_limit - 1,
                ) == CheckResult::Validated
            })
        {
            return CheckResult::Validated;
        }

        if let Some(cr) = self.implied_any.get(criteria)
            && cr.iter().any(|c| {
                self.check_criteria(
                    name,
                    version,
                    c,
                    Some(&CriteriaCons(criteria, implied_criteria)),
                    recursion_limit - 1,
                ) == CheckResult::Validated
            })
        {
            return CheckResult::Validated;
        }

        CheckResult::Missing
    }

    fn find_prev(
        &self,
        name: &str,
        version: &Version,
        criteria: &str,
        recursion_limit: usize,
    ) -> Option<Version> {
        let recursion_limit = recursion_limit.checked_sub(1)?;

        let versions: BTreeSet<_> = self
            .trust_roots
            .values()
            .filter_map(|d| d.get(name))
            .flatten()
            .map(|(v, _)| v)
            .chain(
                self.trust_deltas
                    .values()
                    .filter_map(|d| d.get(name))
                    .flatten()
                    .map(|(v, _)| v),
            )
            .collect();

        for &potential in versions.range::<&Version, _>(..version).rev() {
            if self.check_criteria(name, potential, criteria, None, recursion_limit)
                == CheckResult::Validated
            {
                return Some(potential.clone());
            }
        }

        None
    }

    fn check(&self, name: String, version: Version, recursion_limit: usize) -> Receipt {
        let Policy { require_all } = self.get_policy(&name);

        let fails: Vec<_> = require_all
            .iter()
            .filter_map(
                |c| match self.check_criteria(&name, &version, c, None, recursion_limit) {
                    CheckResult::Validated => None,
                    f => Some(Fail {
                        needed: c.to_string(),
                        reason: match f {
                            CheckResult::Validated => unreachable!(),
                            CheckResult::Missing => FailReason::Missing,
                            CheckResult::RecursionLimitReached => FailReason::RecursionLimitReached,
                            CheckResult::Violation => FailReason::Violation,
                        },
                        prev_version: self.find_prev(&name, &version, c, recursion_limit),
                    }),
                },
            )
            .collect();

        let status = if fails.is_empty() {
            Status::Passed
        } else {
            Status::Failed(fails)
        };

        Receipt {
            name,
            version,
            status,
        }
    }

    fn unused_exempts(&self) -> BTreeSet<UnusedExempt> {
        let mut out = self.extra_unused.clone();

        for (criteria, map) in &self.trust_roots {
            for (dep, map) in map {
                for (version, root) in map {
                    if root
                        .used
                        .0
                        .as_ref()
                        .is_some_and(|b| !b.load(Ordering::Relaxed))
                    {
                        out.insert(UnusedExempt {
                            name: dep.clone(),
                            version: version.to_string(),
                            criteria: criteria.clone(),
                        });
                    }
                }
            }
        }

        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct UnusedExempt {
    name: String,
    version: String,
    criteria: String,
}

fn parse_delta(delta: &str) -> Result<(Version, Version), Error> {
    let Some((prev, next)) = delta.split_once("->") else {
        return Err(Error::ParseDelta(delta.to_string()));
    };

    Ok((
        Version::new(prev.trim_ascii()),
        Version::new(next.trim_ascii()),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum CheckResult {
    Validated,
    Missing,
    RecursionLimitReached,
    Violation,
}

#[derive(Debug, Clone, Serialize)]
enum FailReason {
    Missing,
    RecursionLimitReached,
    Violation,
}

#[derive(Debug, Clone, Serialize)]
struct Fail {
    needed: String,
    reason: FailReason,
    prev_version: Option<Version>,
}

#[derive(Debug, Clone)]
enum Status {
    Passed,
    Failed(Vec<Fail>),
}

impl Serialize for Status {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Passed => {
                let mut s = serializer.serialize_struct("Status", 1)?;
                s.serialize_field("status", "passed")?;
                s.skip_field("fails")?;
                s.end()
            }
            Self::Failed(fails) => {
                let mut s = serializer.serialize_struct("Status", 2)?;
                s.serialize_field("status", "failed")?;
                s.serialize_field("fails", fails)?;
                s.end()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Receipt {
    name: String,
    version: Version,
    #[serde(flatten)]
    status: Status,
}

#[allow(clippy::too_many_lines)]
pub fn do_check(args: &crate::CheckArgs) -> Result<ExitCode, Error> {
    let dependencies = crate::metadata::get_dependencies(&args.lock)?;
    if dependencies.is_empty() {
        return Err(Error::EmptyDependencies);
    }

    let config = read_to_string(&args.config).map_err(Error::ConfigOpen)?;
    let mut config: Config = toml_edit::de::from_str(&config)?;
    let audits = read_to_string(&args.audits).map_err(Error::AuditsOpen)?;
    let audits: Audits = toml_edit::de::from_str(&audits)?;

    if args.ignore_exempts {
        config.exempt.clear();
    }

    let rules = Rules::new(config, audits)?;

    let receipts: Vec<_> = dependencies
        .into_par_iter()
        .map(|(name, version)| rules.check(name, version, args.recursion_limit))
        .collect();
    let total = receipts.len();
    let unused = rules.unused_exempts();

    if args.output == OutputFormat::Json {
        let passed = receipts
            .iter()
            .filter(|&r| matches!(r.status, Status::Passed))
            .count();
        println!(
            "{}",
            json!({
                "dependencies": receipts,
                "total": total,
                "total_failed": total - passed,
                "total_passed": passed,
                "unused_exempts": unused,
            })
        );
    }

    let fails: Vec<_> = receipts
        .into_iter()
        .filter(|r| !matches!(r.status, Status::Passed))
        .collect();

    if args.ratchet && !unused.is_empty() {
        let mut file = open_config(args)?;
        let mut toml: DocumentMut = config_mut(&file)?;

        ratchet_exempts(&unused, &mut toml)?;

        write_config(&mut file, toml.to_string().as_bytes())?;
        eprintln!("removed {} unused exempts :3", unused.len());
    } else if args.output == OutputFormat::Human {
        for UnusedExempt {
            name,
            version,
            criteria,
        } in unused
        {
            println!("unused exempt: {name} {version} {criteria}");
        }
    }

    if fails.is_empty() {
        eprintln!("all {total} crates ok");
        return Ok(ExitCode::SUCCESS);
    }

    if args.add_exempts {
        let mut file = open_config(args)?;
        let mut toml: DocumentMut = config_mut(&file)?;

        add_exempts(&fails, &mut toml)?;

        write_config(&mut file, toml.to_string().as_bytes())?;

        eprintln!("added {} exempts to the config", fails.len());
        return Ok(ExitCode::from(3));
    }

    if args.output == OutputFormat::Human {
        for Receipt {
            name,
            version,
            status,
        } in &fails
        {
            println!("{name} {version}");
            match status {
                Status::Passed => unreachable!(),
                Status::Failed(v) => {
                    for Fail {
                        needed,
                        reason,
                        prev_version,
                    } in v
                    {
                        println!(" needs {needed}");
                        match reason {
                            FailReason::Missing => {
                                if let Some(prev) = prev_version {
                                    println!("  help: found a previous audit for {prev}");
                                    println!("  review https://diff.rs/{name}/{prev}/{version}");
                                    println!(
                                        "  then vancouver audit {name} -b {prev} {version} {needed}"
                                    );
                                } else {
                                    println!("  help: could not find previous audits :(");
                                    println!("  review https://diff.rs/browse/{name}/{version}");
                                    println!("  then vancouver audit {name} {version} {needed}");
                                }
                            }
                            FailReason::RecursionLimitReached => {
                                println!("  recursion limit reached!");
                                if prev_version.is_some() {
                                    println!("  help: try doing a full audit of a newer version");
                                    println!("  or increase the limit with --recursion-limit");
                                } else {
                                    println!("  help: no acceptable previous audits");
                                    println!("  you might have a loop of criteria implies");
                                    println!("  or delta audits somewhere");
                                }
                            }
                            FailReason::Violation => {
                                println!("  matched violation in audits file!");
                                if let Some(prev) = prev_version {
                                    println!("  help: found a previous audit for {prev}");
                                    println!("  consider downgrading to that version");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    eprintln!("{}/{total} crates need to be audited", fails.len());
    Ok(ExitCode::FAILURE)
}

fn write_config(file: &mut std::fs::File, bytes: &[u8]) -> Result<(), Error> {
    file.rewind().map_err(Error::ConfigWrite)?;
    file.set_len(0).map_err(Error::ConfigWrite)?;
    file.write_all(bytes).map_err(Error::ConfigWrite)?;
    Ok(())
}

fn config_mut(file: &std::fs::File) -> Result<DocumentMut, Error> {
    Ok(std::io::read_to_string(file)
        .map_err(Error::ConfigOpen)?
        .parse()?)
}

fn open_config(args: &crate::CheckArgs) -> Result<std::fs::File, Error> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&args.config)
        .map_err(Error::ConfigOpen)?;
    Ok(file)
}

fn add_exempts(fails: &Vec<Receipt>, toml: &mut DocumentMut) -> Result<(), Error> {
    let Item::Table(etable) = toml
        .entry("exempt")
        .or_insert_with(|| Item::Table(Table::new()))
    else {
        return Err(Error::TomlBorked);
    };
    if etable.is_empty() {
        etable.set_implicit(true);
    }

    for Receipt {
        name,
        version,
        status,
    } in fails
    {
        let Item::ArrayOfTables(arr) = etable
            .entry(name)
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        else {
            return Err(Error::TomlBorked);
        };

        match status {
            Status::Passed => unreachable!(),
            Status::Failed(f) => {
                for Fail { needed, .. } in f {
                    let mut t = Table::new();
                    t["version"] = value(version.to_string());
                    t["criteria"] = value(needed);
                    arr.push(t);
                }
            }
        }
    }

    Ok(())
}

fn ratchet_exempts(unused: &BTreeSet<UnusedExempt>, toml: &mut DocumentMut) -> Result<(), Error> {
    let Item::Table(etable) = toml
        .entry("exempt")
        .or_insert_with(|| Item::Table(Table::new()))
    else {
        return Err(Error::TomlBorked);
    };

    etable.retain(|dep, inner| {
        let Item::ArrayOfTables(inner) = inner else {
            return true;
        };

        inner.retain(|t| {
            let Some(Item::Value(Value::String(v))) = t.get("version") else {
                return true;
            };
            let Some(Item::Value(Value::String(c))) = t.get("criteria") else {
                return true;
            };
            !unused.contains(&UnusedExempt {
                name: dep.to_string(),
                version: v.value().clone(),
                criteria: c.value().clone(),
            })
        });

        !inner.is_empty()
    });

    Ok(())
}

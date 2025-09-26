// SPDX-FileCopyrightText: 2025 xfnw
//
// SPDX-License-Identifier: MPL-2.0

use crate::types::{Error, Version};
use rayon::prelude::*;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::read_to_string,
    process::ExitCode,
    sync::atomic::{AtomicBool, Ordering},
};

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
    #[serde(default)]
    implies: BTreeSet<String>,
    /// automatically imply this criteria on everything that has all
    /// of the listed criteria
    #[serde(default, alias = "implied-all")]
    implied_all: BTreeSet<String>,
    /// automatically imply this criteria on everything that has any
    /// of the listed criteria
    #[serde(default, alias = "implied-any")]
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
    used: UsedMarker,
    parent_version: Version,
}

type CriteriaMap<T> = BTreeMap<String, T>;
type DepMap<T> = BTreeMap<String, T>;

#[derive(Debug)]
struct Rules {
    trust_roots: CriteriaMap<DepMap<BTreeMap<Version, TrustRoot>>>,
    trust_deltas: CriteriaMap<DepMap<BTreeMap<Version, TrustDelta>>>,
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

        let mut implies = BTreeMap::new();
        let mut implied_all = BTreeMap::new();
        let mut implied_any = BTreeMap::new();

        for (
            criteria,
            Criteria {
                implies: imp,
                implied_all: all,
                implied_any: any,
            },
        ) in criteria
        {
            implies.insert(criteria.clone(), imp);
            implied_all.insert(criteria.clone(), all);
            implied_any.insert(criteria, any);
        }

        let mut trust_roots: CriteriaMap<DepMap<BTreeMap<Version, TrustRoot>>> = BTreeMap::new();
        let mut trust_deltas: CriteriaMap<DepMap<BTreeMap<Version, TrustDelta>>> = BTreeMap::new();

        // TODO: squish this into a macro
        for (name, aset) in config.exempt {
            for Audit {
                criteria,
                delta,
                version,
                allow_unused,
            } in aset
            {
                walk_implies(&implies, &criteria, |criteria| {
                    if let Some(delta) = &delta {
                        let (prev, next) = parse_delta(delta)?;
                        trust_deltas
                            .entry(criteria.to_string())
                            .or_default()
                            .entry(name.clone())
                            .or_default()
                            .insert(
                                next,
                                TrustDelta {
                                    used: UsedMarker(Some(allow_unused.into())),
                                    parent_version: prev,
                                },
                            );
                    }
                    if let Some(version) = &version {
                        trust_roots
                            .entry(criteria.to_string())
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
                    Ok(())
                })?;
            }
        }
        for (name, aset) in audits.audits {
            for Audit {
                criteria,
                delta,
                version,
                ..
            } in aset
            {
                walk_implies(&implies, &criteria, |criteria| {
                    if let Some(delta) = &delta {
                        let (prev, next) = parse_delta(delta)?;
                        trust_deltas
                            .entry(criteria.to_string())
                            .or_default()
                            .entry(name.clone())
                            .or_default()
                            .insert(
                                next,
                                TrustDelta {
                                    used: UsedMarker(None),
                                    parent_version: prev,
                                },
                            );
                    }
                    if let Some(version) = &version {
                        trust_roots
                            .entry(criteria.to_string())
                            .or_default()
                            .entry(name.clone())
                            .or_default()
                            .insert(
                                Version::new(version),
                                TrustRoot {
                                    used: UsedMarker(None),
                                },
                            );
                    }
                    Ok(())
                })?;
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
        recursion_limit: usize,
    ) -> bool {
        if let Some(trust) = self
            .trust_roots
            .get(criteria)
            .and_then(|d| d.get(name))
            .and_then(|v| v.get(version))
        {
            trust.used.mark_used();
            return true;
        }

        if recursion_limit == 0 {
            return false;
        }

        if let Some(trust) = self
            .trust_deltas
            .get(criteria)
            .and_then(|d| d.get(name))
            .and_then(|v| v.get(version))
        {
            trust.used.mark_used();
            return self.check_criteria(name, &trust.parent_version, criteria, recursion_limit - 1);
        }

        if let Some(criteria) = self.implied_all.get(criteria)
            && criteria
                .iter()
                .all(|c| self.check_criteria(name, version, c, recursion_limit - 1))
        {
            return true;
        }

        if let Some(criteria) = self.implied_any.get(criteria)
            && criteria
                .iter()
                .any(|c| self.check_criteria(name, version, c, recursion_limit - 1))
        {
            return true;
        }

        false
    }

    fn check(&self, name: String, version: Version, recursion_limit: usize) -> Receipt {
        let Policy { require_all } = self.get_policy(&name);

        let fails: Vec<_> = require_all
            .iter()
            .filter_map(|c| {
                if self.check_criteria(&name, &version, c, recursion_limit) {
                    None
                } else {
                    Some(Fail {
                        needed: c.to_string(),
                        // TODO: search for a previous version's audit
                        prev_version: None,
                    })
                }
            })
            .collect();

        let status = if fails.is_empty() {
            Status::Passed
        } else {
            Status::Fail(fails)
        };

        Receipt {
            name,
            version,
            status,
        }
    }
}

fn walk_implies(
    implies: &BTreeMap<String, BTreeSet<String>>,
    c: &str,
    mut f: impl FnMut(&str) -> Result<(), Error>,
) -> Result<(), Error> {
    f(c)?;

    if let Some(criteria) = implies.get(c) {
        for c in criteria {
            f(c)?;
        }
    }

    Ok(())
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

#[derive(Debug, Clone)]
struct Fail {
    needed: String,
    prev_version: Option<Version>,
}

#[derive(Debug, Clone)]
enum Status {
    Passed,
    Fail(Vec<Fail>),
}

#[derive(Debug, Clone)]
struct Receipt {
    name: String,
    version: Version,
    status: Status,
}

pub fn do_check(args: &crate::CheckArgs) -> Result<ExitCode, Error> {
    let dependencies = crate::metadata::get_dependencies(&args.lock)?;
    if dependencies.is_empty() {
        return Err(Error::EmptyDependencies);
    }

    let config = read_to_string(&args.config).map_err(Error::ConfigOpen)?;
    let config: Config = toml_edit::de::from_str(&config)?;
    let audits = read_to_string(&args.audits).map_err(Error::AuditsOpen)?;
    let audits: Audits = toml_edit::de::from_str(&audits)?;
    let rules = Rules::new(config, audits)?;

    let receipts: Vec<_> = dependencies
        .into_par_iter()
        .map(|(name, version)| rules.check(name, version, args.recursion_limit))
        .collect();
    let total = receipts.len();
    let fails: Vec<_> = receipts
        .into_iter()
        .filter(|r| !matches!(r.status, Status::Passed))
        .collect();

    if fails.is_empty() {
        // TODO: check for unused exempts here
        eprintln!("all {total} crates ok");
        return Ok(ExitCode::SUCCESS);
    }

    // TODO: add flag to add exempts in the config for failures

    for Receipt {
        name,
        version,
        status,
    } in &fails
    {
        println!("{name} {version}");
        match status {
            Status::Passed => unreachable!(),
            Status::Fail(v) => {
                for Fail {
                    needed,
                    prev_version,
                } in v
                {
                    println!(" needs {needed}");
                    if let Some(prev) = prev_version {
                        println!("  help: found a previous audit for {prev}");
                        println!("  review https://diff.rs/{name}/{prev}/{version}");
                        println!("  and then vancouver audit {needed} {name} {prev} {version}");
                    } else {
                        println!("  help: could not find previous audits :(");
                        println!("  review https://diff.rs/browse/{name}/{version}");
                        println!("  and then vancouver audit {needed} {name} {version}");
                    }
                }
            }
        }
    }

    eprintln!("{}/{total} crates need to be audited", fails.len());
    Ok(ExitCode::FAILURE)
}

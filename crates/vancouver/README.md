<!--
SPDX-FileCopyrightText: 2025 xfnw

SPDX-License-Identifier: MPL-2.0
-->

# vancouver
dependency auditing that meows

it is like cargo-vet but kitty cat sized

## is this cargo-vet compatible?
not really. while vancouver's `audits.toml` is superficially similar
to cargo-vet's, the semantics and supported features are different.
for example, vancouver does not support wildcard audits and ignores
many fields that cargo-vet requires, while cargo-vet does not support
some of vancouver's more advanced criteria implications.

it should be possible to write an `audits.toml` that works with both
cargo-vet and vancouver, just do not expect that to happen
automatically. 

## did you really name this after the cat youtuber?
... maybe

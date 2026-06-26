<!--
SPDX-FileCopyrightText: 2025 xfnw

SPDX-License-Identifier: MPL-2.0
-->

# changelog

## unreleased

### changed
- check now links directly to the docs.rs source view instead of
  diff.rs when suggesting a full audit
- check will no longer suggest doing delta audits against a previous
  exempted version

### added
- check now has a --no-suggest-delta option to never suggest doing
  delta audits

## 0.1.2 - 2025-12-13

### fixed
- duplicate full audits will no longer cause spurious unused exempt
  warnings

### changed
- check now includes the reason a dependency failed verification in
  its error messages
- the failure reason is also included in check's json output

### added
- violation audits are now supported for specific versions (but not a
  range of versions)

## 0.1.1 - 2025-10-15

### fixed
- version sorting to find a previous audited version is now in the
  correct order
- audits are no longer required for dependencies from unsupported
  sources

## 0.1.0 - 2025-10-02
initial release

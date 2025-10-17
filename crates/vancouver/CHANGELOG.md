<!--
SPDX-FileCopyrightText: 2025 xfnw

SPDX-License-Identifier: MPL-2.0
-->

# changelog

## unreleased

### fixes
- duplicate full audits will no longer cause spurious unused exempt
  warnings

### changes
- check now includes the reason a dependency failed verification in
  its error messages
- the failure reason is also included in check's json output

### added
- violation audits are now supported for specific versions (but not a
  range of versions)

## 0.1.1 - 2025-10-15

### fixes
- version sorting to find a previous audited version is now in the
  correct order
- audits are no longer required for dependencies from unsupported
  sources

## 0.1.0 - 2025-10-02
initial release

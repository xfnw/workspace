<!--
SPDX-FileCopyrightText: 2025 xfnw

SPDX-License-Identifier: MIT
-->

# changelog

## 0.2.1 - 2026-05-29
- `Stream` has been renamed to `Connection`, since the name was easily
  confusable with `futures_core::Stream`. using the old name is
  deprecated
- `StreamBuilder` has been renamed to `ConnectionBuilder`. using the
  old name is deprecated

## 0.2.0 - 2025-07-29
- `Stream::new_tcp` now accepts anything implementing `IntoTargetAddr`
- socks behavior switched around and now requires a `SocketAddr`
- `StreamBuilder` has been marked as `must_use`
- giving tls builder methods a `ServerName` is now optional

## 0.1.1 - 2025-07-28
- fix documentation for socks
- mark current socks behavior as deprecated

## 0.1.0 - 2025-07-20
- initial release

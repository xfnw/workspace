# changelog

## unreleased
- `Stream::new_tcp` now accepts anything implementing `IntoTargetAddr`
- socks behavior switched around and now requires a `SocketAddr`
- `StreamBuilder` has been marked as `must_use`

## 0.1.1 - 2025-07-28
- fix documentation for socks
- mark current socks behavior as deprecated

## 0.1.0 - 2025-07-20
- initial release

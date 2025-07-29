# changelog

## unreleased
- `Stream::new_tcp` now requires a `SocketAddr` instead of a
  `&SocketAddr`, since it is `Copy`
- socks behavior switched around and now requires a `SocketAddr`

## 0.1.1 - 2025-07-28
- fix documentation for socks
- mark current socks behavior as deprecated

## 0.1.0 - 2025-07-20
- initial release

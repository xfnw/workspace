// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: MIT

pub trait FileStore<const D: usize>: Send + Sync + 'static {
    type Error: std::error::Error + Send + 'static;

    fn store(&self, file: &[u8]) -> impl Future<Output = Result<[u8; D], Self::Error>> + Send;

    fn retrieve(
        &self,
        digest: [u8; D],
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send;

    fn shutdown(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

// SPDX-FileCopyrightText: 2026 xfnw
//
// SPDX-License-Identifier: EUPL-1.2

use std::{
    io::Error,
    pin::Pin,
    task::{Context, Poll, ready},
};

use hyper::body::{Body, Bytes, Frame, SizeHint};
use pin_project_lite::pin_project;
use tokio::io::AsyncBufRead;

pin_project! {
    pub struct BufReadExactBody<R> {
        #[pin]
        reader: R,
        length: u64,
    }
}

impl<R> BufReadExactBody<R> {
    pub fn new(reader: R, length: u64) -> Self {
        Self { reader, length }
    }
}

impl<R> Body for BufReadExactBody<R>
where
    R: AsyncBufRead,
{
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let mut project = self.project();

        if *project.length == 0 {
            return Poll::Ready(None);
        }

        let data = match ready!(project.reader.as_mut().poll_fill_buf(cx)) {
            Ok(d) => d,
            Err(e) => return Poll::Ready(Some(Err(e))),
        };

        let consumed = std::cmp::min(data.len() as u64, *project.length);
        *project.length -= consumed;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "result of min with a value that was previously usize should fit in usize"
        )]
        let consumed = consumed as usize;

        // it'd be nice if we could return references to the BufReader's buffer directly,
        // but this vulpine has no idea how to do that :(
        let data = Bytes::copy_from_slice(&data[0..consumed]);

        project.reader.consume(consumed);

        Poll::Ready(Some(Ok(Frame::data(data))))
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.length)
    }
}

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_lite::{
    AsyncWrite,
    io::{self, Result},
    ready,
};

use crate::BUFFER_SIZE;

pub trait MapWriteFn {
    fn map_write(&mut self, buf: &mut Vec<u8>);
}

impl<F> MapWriteFn for F
where
    F: FnMut(&mut Vec<u8>),
{
    fn map_write(&mut self, buf: &mut Vec<u8>) {
        self(buf)
    }
}

pin_project_lite::pin_project! {
  pub struct AsyncMapWriter<'a, W> {
     #[pin]
     inner: W,
     process_fn: Box<dyn MapWriteFn + 'a>,
     buf: Vec<u8>,
     written: usize,
     transformed: bool, // Add a flag to track if the buffer is already transformed
  }
}

impl<'a, W: AsyncWrite> AsyncMapWriter<'a, W> {
    pub fn new(writer: W, process_fn: impl MapWriteFn + 'a) -> Self {
        Self::with_capacity(writer, process_fn, BUFFER_SIZE)
    }

    pub fn with_capacity(writer: W, process_fn: impl MapWriteFn + 'a, capacity: usize) -> Self {
        Self {
            inner: writer,
            process_fn: Box::new(process_fn),
            buf: Vec::with_capacity(capacity),
            written: 0,
            transformed: false,
        }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut W> {
        self.project().inner
    }

    fn poll_flush_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut this = self.project();
        // If nothing has been written yet and the buffer isn't transformed, apply the transformation
        if *this.written == 0 && !this.buf.is_empty() && !*this.transformed {
            (this.process_fn).map_write(this.buf);
            *this.transformed = true; // Mark as transformed
        }
        let len = this.buf.len();
        let mut ret = Ok(());

        while *this.written < len {
            match this
                .inner
                .as_mut()
                .poll_write(cx, &this.buf[*this.written..])
            {
                Poll::Ready(Ok(0)) => {
                    ret = Err(io::Error::new(io::ErrorKind::WriteZero, "write zero"));
                    break;
                }
                Poll::Ready(Ok(n)) => {
                    *this.written += n;
                }
                Poll::Ready(Err(ref e)) if e.kind() == io::ErrorKind::Interrupted => {}
                Poll::Ready(Err(e)) => {
                    ret = Err(e);
                    break;
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }

        if *this.written > 0 {
            this.buf.drain(..*this.written);
        }
        *this.written = 0;
        *this.transformed = false; // Reset transformed flag when buffer is drained

        Poll::Ready(ret)
    }

    fn large_write(self: Pin<&mut Self>, buf: &[u8]) {
        let this = self.project();
        // Clear any leftover data.
        this.buf.clear();
        // Ensure the buffer can hold the new data.
        if buf.len() > this.buf.capacity() {
            this.buf.reserve(buf.len() - this.buf.capacity());
        }
        // Copy and process the new data into our internal buffer.
        this.buf.extend_from_slice(buf);
        (this.process_fn).map_write(this.buf);
        *this.transformed = true; // Mark as transformed since we've already applied the function
    }
}

impl<W: AsyncWrite> AsyncWrite for AsyncMapWriter<'_, W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize>> {
        // Flush the internal buffer if adding new data would exceed capacity.
        if self.buf.len() + buf.len() > self.buf.capacity() {
            ready!(self.as_mut().poll_flush_buf(cx))?;
        }

        if buf.len() < self.buf.capacity() {
            // For small writes, write into our internal buffer so that the
            // mapping function is applied later in poll_flush_buf.
            return Pin::new(&mut *self.project().buf).poll_write(cx, buf);
        }
        // If data is large, process it before writing using the internal buffer.
        self.as_mut().large_write(buf);

        // Instead of attempting to write immediately and potentially leaving
        // data behind, we'll just report that all input bytes were consumed.
        // The transformed data will be completely written during flush.
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        ready!(self.as_mut().poll_flush_buf(cx))?;
        self.get_pin_mut().poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        ready!(self.as_mut().poll_flush_buf(cx))?;
        self.get_pin_mut().poll_close(cx)
    }
}

pub trait AsyncMapWrite<'a, W> { 
    fn map(self, process_fn: impl MapWriteFn + 'a) -> AsyncMapWriter<'a, W>;
}

impl<'a, W: AsyncWrite> AsyncMapWrite<'a, W> for W {
    fn map(self, process_fn: impl MapWriteFn + 'a) -> AsyncMapWriter<'a, W> {
        AsyncMapWriter::new(self, process_fn)
    }
}
use crate::DEFAULT_BUFFER_SIZE;
use futures_lite::{io, ready, AsyncBufRead, AsyncRead};
use std::pin::Pin;
use std::task::{Context, Poll};

/// A trait for mapping data read from an underlying reader.
pub trait MapReadFn {
    /// Applies a mapping function to the data read from the underlying reader.
    /// This function takes a mutable reference to a buffer and modifies it in place.
    ///
    /// The length of the buffer is gauranteed to be equal to the capacity of the underlying
    /// buffer, until the last read operation, where it may be smaller.
    fn map_read(&mut self, buf: &mut [u8]);
}

impl<F> MapReadFn for F
where
    F: FnMut(&mut [u8]),
{
    fn map_read(&mut self, buf: &mut [u8]) {
        self(buf)
    }
}

pin_project_lite::pin_project! {
  /// A wrapper around an `AsyncRead` that allows for data processing
  /// before the actual I/O operation.
  /// 
  /// This struct buffers the data read from the underlying reader and applies a mapping function
  /// to the data before returning it. It is designed to optimize reads by using a buffer
  /// of a specified size (default is 8KB).
  /// 
  /// The buffer size also acts as a threshold for the length of data passed to the mapping function,
  /// and will be gauranteed to be equal the capacity of the underlying buffer, until the last read operation,
  /// where it may be smaller.
  pub struct AsyncMapReader<'a, R> {
      #[pin]
      inner: R,
      process_fn: Box<dyn MapReadFn + 'a>,
      pos: usize, // Current position in the buffer
      cap: usize, // Current position and capacity of the buffer
      buf: Box<[u8]>, // Internal buffer for reading data
  }
}

impl<'a, R> AsyncMapReader<'a, R>
where
    R: AsyncRead,
{
    /// Create a new wrapper around an async reader with a processing function
    pub fn new(reader: R, process_fn: impl MapReadFn + 'a) -> Self {
        Self {
            inner: reader,
            process_fn: Box::new(process_fn),
            pos: 0,
            cap: 0,
            buf: vec![0; DEFAULT_BUFFER_SIZE].into_boxed_slice(), // Start with a reasonable capacity
        }
    }

    /// Create a new wrapper with a specific initial buffer capacity
    pub fn with_capacity(reader: R, process_fn: impl MapReadFn + 'a, capacity: usize) -> Self {
        Self {
            inner: reader,
            process_fn: Box::new(process_fn),
            pos: 0,
            cap: 0,
            buf: vec![0; capacity].into_boxed_slice(),
        }
    }

    /// Consume the wrapper and return the inner reader
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<'a, R> AsyncRead for AsyncMapReader<'a, R>
where
    R: AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if self.pos == self.cap {
            let fill = ready!(self.as_mut().poll_fill_buf(cx))?;
            if fill.is_empty() {
                return Poll::Ready(Ok(0));
            }
        }
        let rem = {
            let this = self.as_mut().project();
            &this.buf[*this.pos..*this.cap]
        };
        let amt = std::cmp::min(rem.len(), buf.len());
        buf[..amt].copy_from_slice(&rem[..amt]);
        self.consume(amt);
        Poll::Ready(Ok(amt))
    }
}

impl<'a, R: AsyncRead> AsyncBufRead for AsyncMapReader<'a, R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        let mut this = self.project();
        if *this.pos >= *this.cap {
            debug_assert!(*this.pos == *this.cap);
            *this.pos = 0;
            *this.cap = 0;
            let read_amount = ready!(this.inner.as_mut().poll_read(cx, this.buf))?;
            if read_amount == 0 {
                return Poll::Ready(Ok(&[]));
            }
            (this.process_fn).map_read(&mut this.buf[..read_amount]);
            *this.cap = read_amount;
        }
        Poll::Ready(Ok(&this.buf[*this.pos..*this.cap]))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let this = self.project();
        *this.pos = std::cmp::min(*this.pos + amt, *this.cap);
    }
}

/// A trait for types that can be mapped to an `AsyncMapReader`.
pub trait AsyncMapRead<'a, R> {
    /// Maps the underlying reader to an `AsyncMapReader` using the provided mapping function.
    /// This function uses a default buffer size (8KB) for the internal buffer.
    fn map(self, f: impl MapReadFn + 'a) -> AsyncMapReader<'a, R>
    where
        Self: Sized,
    {
        self.map_with_capacity(f, DEFAULT_BUFFER_SIZE)
    }

    /// Maps the underlying reader to an `AsyncMapReader` using the provided mapping function
    /// and a specified buffer capacity.
    ///
    /// This function allows for more control over the internal buffer size, which can be useful
    /// for performance tuning.
    fn map_with_capacity(self, f: impl MapReadFn + 'a, capacity: usize) -> AsyncMapReader<'a, R>;
}

impl<'a, R: AsyncRead> AsyncMapRead<'a, R> for R {
    fn map_with_capacity(self, f: impl MapReadFn + 'a, capacity: usize) -> AsyncMapReader<'a, R> {
        AsyncMapReader::with_capacity(self, f, capacity)
    }
}

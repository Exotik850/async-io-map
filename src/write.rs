use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_lite::{
    io::{self, Result},
    ready, AsyncWrite,
};

use crate::BUFFER_SIZE;

/// A trait for mapping data written to an underlying writer.
pub trait MapWriteFn {
    /// Applies a mapping function to the data before writing it to the underlying writer.
    /// This function takes a mutable reference to a buffer and modifies it in place.
    /// 
    /// Be aware that changing the capacity of the buffer will affect any subsequent writes,
    /// if this is not intended, ensure to reset the capacity of the buffer after processing.
    /// 
    /// This behavior is intended to allow for a variety of use cases, such as base64 encoding,
    /// which may require expanding the buffer size to accommodate the transformed data.
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
  /// A wrapper around an `AsyncWrite` that allows for data processing
  /// before the actual I/O operation.
  /// 
  /// This struct buffers the data written to the underlying writer and applies a mapping function
  /// to the data before writing it out. It is designed to optimize writes by using a buffer
  /// of a specified size (default is 8KB).
  /// 
  /// The buffer size also acts as a threshold for the length of data passed to the mapping function, 
  /// and will be gauranteed to be equal to or less than the specified capacity.
  pub struct AsyncMapWriter<'a, W> {
     #[pin]
     inner: W,
     process_fn: Box<dyn MapWriteFn + 'a>,
     buf: Vec<u8>, // Buffer to hold data before writing
     written: usize, // Track how much has been written to the buffer
     transformed: bool, // Add a flag to track if the buffer is already transformed
  }
}

impl<'a, W: AsyncWrite> AsyncMapWriter<'a, W> {
    /// Creates a new `AsyncMapWriter` with a default buffer size of 8KB.
    /// 
    /// This function initializes the writer with the provided `process_fn` to map the data before writing.
    pub fn new(writer: W, process_fn: impl MapWriteFn + 'a) -> Self {
      Self::with_capacity(writer, process_fn, BUFFER_SIZE)
    }
    
    /// Creates a new `AsyncMapWriter` with a specified buffer capacity.
    /// 
    /// This function initializes the writer with the provided `process_fn` to map the data before writing.
    pub fn with_capacity(writer: W, process_fn: impl MapWriteFn + 'a, capacity: usize) -> Self {
        Self {
            inner: writer,
            process_fn: Box::new(process_fn),
            buf: Vec::with_capacity(capacity),
            written: 0,
            transformed: false,
        }
    }

    /// Consumes the `AsyncMapWriter` and returns the underlying writer.
    pub fn into_inner(self) -> W {
        self.inner
    }

    fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut W> {
        self.project().inner
    }

    /// Flushes the internal buffer, applying the mapping function if necessary.
    /// This function writes the transformed data to the underlying writer.
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

    /// Handles large writes by processing the data before writing it to the underlying writer.
    /// This function ensures that the internal buffer is transformed before writing.
    /// 
    /// returns the number of bytes written to the internal buffer.
    fn partial_write(self: Pin<&mut Self>, buf: &[u8]) -> usize {
        let this = self.project();
        debug_assert!(
            !*this.transformed,
            "large_write should only be called when the buffer is not transformed"
        );
        // Determine how many bytes can fit into the unused part of the internal buffer.
        let available = this.buf.capacity() - this.buf.len();
        let to_read = available.min(buf.len());

        // Only append if there's space.
        if to_read > 0 {
            this.buf.extend_from_slice(&buf[..to_read]);
            // If not yet transformed, process the accumulated data.
            if !*this.transformed {
                (this.process_fn).map_write(this.buf);
                *this.transformed = true;
            }
        }
        to_read
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
        let read = self.as_mut().partial_write(buf);

        // Instead of attempting to write immediately and potentially leaving
        // data behind, we'll just report however many bytes we've processed
        // so far.
        Poll::Ready(Ok(read))
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
    /// Maps the data written to the writer using the provided function.
    /// 
    /// This function will apply the mapping function to the data before writing it to the underlying writer.
    /// This also buffers the data (with a buffer size of 8KB) to optimize writes.
    fn map(self, process_fn: impl MapWriteFn + 'a) -> AsyncMapWriter<'a, W>
    where
        Self: Sized,
    {
        self.map_with_capacity(process_fn, BUFFER_SIZE)
    }

    /// Maps the data written to the writer using the provided function with a specified buffer capacity.
    /// 
    /// This function allows you to specify the size of the internal buffer used for writing.
    /// The default buffer size is 8KB.
    /// If you need to optimize for larger writes, you can increase this size.
    fn map_with_capacity(
        self,
        process_fn: impl MapWriteFn + 'a,
        capacity: usize,
    ) -> AsyncMapWriter<'a, W>;
}

impl<'a, W: AsyncWrite> AsyncMapWrite<'a, W> for W {
    fn map_with_capacity(
        self,
        process_fn: impl MapWriteFn + 'a,
        capacity: usize,
    ) -> AsyncMapWriter<'a, W> {
        AsyncMapWriter::with_capacity(self, process_fn, capacity)
    }
}

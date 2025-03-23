mod read;
mod write;

pub use read::*;
pub use write::*;

#[cfg(test)]
mod test;

const DEFAULT_BUFFER_SIZE: usize = 8192;

// TODO - Would it be better to use a dynamic buffer that grows for the reader, 
// or to change the writer to use a fixed size buffer and flush it when full like the reader?
// This would allow for more efficient writes and reads, especially for larger data sets,
// but would require more complex logic to manage the buffer size and flushing behavior.
// Conversely, the current implementation of the writer allows for more user error 
// by allowing the user to expand the buffer capacity which will affect future writes.
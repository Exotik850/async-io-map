use std::sync::atomic::AtomicI32;

use futures_lite::{future::block_on, io::Cursor, AsyncWriteExt};

use crate::write::AsyncMapWriter;

#[test]
fn basic_transformation() {
    let output = Cursor::new(vec![]);
    // Create a transformation that converts lowercase to uppercase
    let transformer = |buf: &mut Vec<u8>| {
        for byte in buf.iter_mut() {
            if *byte >= b'a' && *byte <= b'z' {
                *byte = *byte - b'a' + b'A';
            }
        }
    };

    let mut writer = AsyncMapWriter::new(output, transformer);
    block_on(async {
        writer.write_all(b"hello world").await.unwrap();
        writer.flush().await.unwrap();

        let result = writer.into_inner().into_inner();
        assert_eq!(result, b"HELLO WORLD");
    });
}

#[test]
fn small_writes_accumulation() {
    let output = Cursor::new(vec![]);
    let counter = AtomicI32::new(0);
    // Count how many times transformation is applied
    let transformer = |_: &mut Vec<u8>| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    };

    let mut writer = AsyncMapWriter::with_capacity(output, transformer, 10);
    block_on(async {
        writer.write_all(b"abc").await.unwrap(); // Should buffer, not write yet
        writer.write_all(b"def").await.unwrap(); // Still buffering
        writer.flush().await.unwrap(); // Now it should write and transform

        assert_eq!(writer.into_inner().into_inner(), b"abcdef");
        let counter = counter.load(std::sync::atomic::Ordering::SeqCst);
        assert!(counter == 1, "Transformer should have only been called once");
    });
}

#[test]
fn large_write_exceeding_buffer() {
    let output = Cursor::new(vec![]);
    // Double every byte
    let transformer = |buf: &mut Vec<u8>| {
        let original = buf.clone();
        buf.clear();
        for byte in original {
            buf.push(byte);
            buf.push(byte);
        }
    };

    let mut writer = AsyncMapWriter::with_capacity(output, transformer, 5);
    block_on(async {
        // This write is larger than buffer capacity
        writer.write_all(b"abcdefghij").await.unwrap();
        writer.flush().await.unwrap();

        let result = writer.into_inner().into_inner();
        assert_eq!(result, b"aabbccddeeffgghhiijj");
    });
}

#[test]
fn empty_write() {
    let output = Cursor::new(vec![]);
    let transformer = |_: &mut Vec<u8>| {};

    let mut writer = AsyncMapWriter::new(output, transformer);
    block_on(async {
        let n = writer.write(b"").await.unwrap();
        assert_eq!(n, 0, "Empty write should return 0 bytes written");
        writer.flush().await.unwrap();

        let result = writer.into_inner().into_inner();
        assert!(result.is_empty(), "No data should have been written");
    });
}

#[test]
fn close_behavior() {
    let output = Cursor::new(vec![]);
    // Add a prefix to the data
    let transformer = |buf: &mut Vec<u8>| {
        let mut prefix = b"prefix:".to_vec();
        prefix.append(buf);
        *buf = prefix;
    };

    let mut writer = AsyncMapWriter::new(output, transformer);
    block_on(async {
        writer.write_all(b"data").await.unwrap();
        // Close should flush any remaining data
        writer.close().await.unwrap();

        let result = writer.into_inner().into_inner();
        assert_eq!(result, b"prefix:data");
    });
}

#[test]
fn identity_function() {
    // Test that an identity function (no transformation) works correctly
    let output = Cursor::new(vec![]);
    let transformer = |_: &mut Vec<u8>| {
        // No transformation needed, just return the buffer as is
    };

    let mut writer = AsyncMapWriter::new(output, transformer);
    block_on(async {
        writer.write_all(b"identity test").await.unwrap();
        writer.flush().await.unwrap();

        let result = writer.into_inner().into_inner();
        assert_eq!(result, b"identity test");
    });
}
#[test]
fn chunk_processing_after_large_write() {

  // Create a transformation that reverses the data in each processed chunk.
  // This will help verify that the chunk processing respects the buffer capacity.
  let transformer = |buf: &mut Vec<u8>| {
      println!("Processing chunk");
      buf.reverse();
  };

  // Create output with a small buffer capacity.
  let output = Cursor::new(vec![]);
  // Set a small capacity to force chunking (e.g., 10 bytes).
  let mut writer = AsyncMapWriter::with_capacity(output, transformer, 10);

  block_on(async {
      // Write a large chunk that exceeds the buffer capacity, forcing internal flushes.
      let large_chunk = b"abcdefghijklmno"; // 15 bytes
      writer.write_all(large_chunk).await.unwrap();
      
      // After writing the large chunk, the first 10 bytes ("abcdefghij") will be flushed
      // and reversed to "jihgfedcba", while "klmno" remains in the buffer.
      
      // Now perform many small writes.
      for &byte in b"0123456789" {
          writer.write_all(&[byte]).await.unwrap();
      }
      // The remaining bytes "klmno0123456789" will be chunked
      // into "klmno01234" and "56789", each reversed separately,
      // resulting in "43210onmlk98765".
      writer.flush().await.unwrap();

      let result = writer.into_inner().into_inner();
      let expected = b"jihgfedcba43210onmlk98765";
      assert_eq!(result, expected, "Output should match transformed chunks");
  });
}

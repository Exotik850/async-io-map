use std::sync::atomic::AtomicI32;

use futures_lite::{AsyncWriteExt, future::block_on, io::Cursor};

use crate::write::AsyncMapWriter;

#[test]
fn test_basic_transformation() {
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
fn test_small_writes_accumulation() {
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
        assert!(counter > 0, "Transformer should have been called");
    });
}

#[test]
fn test_large_write_exceeding_buffer() {
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
fn test_empty_write() {
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
fn test_close_behavior() {
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

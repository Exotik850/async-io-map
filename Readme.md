# async-io-map

A lightweight Rust library for transforming data during asynchronous I/O operations.

[![Crates.io](https://img.shields.io/badge/crates.io-v0.1.0-blue)](https://crates.io/crates/async-io-map)
[![Docs.rs](https://docs.rs/async-io-map/badge.svg)](https://docs.rs/async-io-map)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.65%2B-orange)](https://www.rust-lang.org/)

## Table of Contents
- [Overview](#overview)
- [Installation](#installation)
- [Usage](#usage)
- [Features](#features)
- [Configuration](#configuration)
- [Contributing](#contributing)
- [License](#license)
- [Support](#support)

## Overview

`async-io-map` provides wrappers for async readers and writers that allow you to transform data in-flight during I/O operations. The library offers a simple, flexible API that integrates seamlessly with the `futures_lite` ecosystem.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
async-io-map = "0.1.0"
futures-lite = "1.12.0"  # Required peer dependency
```

## Usage

### Reading with Transformation

```rust
use async_io_map::AsyncMapReader;
use futures_lite::io::Cursor;
use futures_lite::io::AsyncReadExt;
use futures_lite::future::block_on;

fn main() {
    // Create a cursor with some data
    let data = b"Hello, World!";
    let cursor = Cursor::new(data.to_vec());

    // Create a reader that converts text to uppercase
    let mut reader = AsyncMapReader::new(cursor, |data: &mut [u8]| {
        data.iter_mut().for_each(|d| d.make_ascii_uppercase());
    });

    // Read from the transformed source
    block_on(async {
        let mut buffer = vec![0u8; 20];
        let bytes_read = reader.read(&mut buffer).await.unwrap();
        
        // Will print: Read 13 bytes: "HELLO, WORLD!"
        println!(
            "Read {} bytes: {:?}",
            bytes_read,
            String::from_utf8_lossy(&buffer[..bytes_read])
        );
    });
}
```

### Writing with Transformation

```rust
use async_io_map::AsyncMapWriter;
use futures_lite::io::Cursor;
use futures_lite::io::AsyncWriteExt;
use futures_lite::future::block_on;

fn main() {
    // Create an output buffer
    let output = Cursor::new(vec![]);
    
    // Create a writer that converts text to uppercase
    let mut writer = AsyncMapWriter::new(output, |data: &mut Vec<u8>| {
        data.iter_mut().for_each(|d| {
            if *d >= b'a' && *d <= b'z' {
                *d = *d - b'a' + b'A';
            }
        });
    });
    
    // Write and transform data
    block_on(async {
        writer.write_all(b"hello world").await.unwrap();
        writer.flush().await.unwrap();
        
        // Get the transformed result
        let result = writer.into_inner().into_inner();
        // Will contain: "HELLO WORLD"
        println!("{}", String::from_utf8_lossy(&result));
    });
}
```

## Features

### 1. Efficient Buffered Operations

The library uses internal buffering to minimize the number of read/write operations to the underlying I/O source, providing optimal performance even with small read/write calls.

### 2. Flexible Transformation API

Transform your data with simple closures or implement the `MapReadFn`/`MapWriteFn` traits for more complex transformations. The transformation logic has full access to modify the data in-place.

### 3. Zero-Copy Design

The library is designed to minimize allocations and copying, with transformations applied directly to buffers before they're passed to the underlying I/O operations.

### 4. Support for Trait Extensions

Extends any type implementing `AsyncRead` or `AsyncWrite` with `.map()` and `.map_with_capacity()` methods for seamless integration with existing code.

## Configuration

Both `AsyncMapReader` and `AsyncMapWriter` can be configured with custom buffer sizes:

```rust
// Create a reader with a 4KB buffer
let reader = AsyncMapReader::with_capacity(source, transform_fn, 4096);

// Create a writer with a 16KB buffer
let writer = AsyncMapWriter::with_capacity(destination, transform_fn, 16384);
```

The default buffer size is 8KB, which works well for most use cases. Adjust based on your expected I/O patterns.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

Please ensure your code passes all tests and follows the project's coding style.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Support

For questions, issues, or feature requests, please open an issue on the [GitHub repository](https://github.com/Exotik850/async-io-map).
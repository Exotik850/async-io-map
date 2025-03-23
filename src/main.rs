use async_io_map::AsyncMapReader;
use futures_lite::io;

// Example usage
fn main() -> io::Result<()> {
    futures_lite::future::block_on(run())
}

async fn run() -> io::Result<()> {
    use futures_lite::io::Cursor;

    // Create a cursor with some data
    let data = b"Hello, World!";
    let cursor = Cursor::new(data.to_vec());

    // Create our wrapper
    let mut reader = AsyncMapReader::new(cursor, |data: &mut [u8]| {
        data.iter_mut().for_each(|d| d.make_ascii_uppercase());
    });

    // Read from it
    let mut buffer = vec![0u8; 20];
    let bytes_read = futures_lite::io::AsyncReadExt::read(&mut reader, &mut buffer).await?;

    // Print the result
    println!(
        "Read {} bytes: {:?}",
        bytes_read,
        String::from_utf8_lossy(&buffer[..bytes_read])
    );

    Ok(())
}

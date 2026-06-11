//! Temporary verification helper: uncart a file to stdout.
//! Usage: uncart_to_stdout <file.cart> [reader_buf_capacity]
use tokio::io::{AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: uncart_to_stdout <file.cart> [reader_buf_capacity]");
    let cap: usize = std::env::args()
        .nth(2)
        .map(|s| s.parse().unwrap())
        .unwrap_or(cart_rs::CART_IO_BUF_SIZE);
    let file = tokio::fs::File::open(path).await?;
    let mut uncart = cart_rs::UncartStream::new(BufReader::with_capacity(cap, file));
    let mut stdout = tokio::io::stdout();
    tokio::io::copy(&mut uncart, &mut stdout).await?;
    stdout.flush().await?;
    Ok(())
}

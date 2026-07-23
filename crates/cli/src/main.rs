use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    releasedock_cli::run().await
}

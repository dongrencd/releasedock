use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    ghrm_cli::run().await
}

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    localproxy::init_tracing();
    localproxy::cli::run().await
}

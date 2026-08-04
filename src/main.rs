use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    zproxy::init_tracing();
    zproxy::cli::run().await
}

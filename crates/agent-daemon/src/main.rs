#[tokio::main]
async fn main() -> anyhow::Result<()> {
    agent_daemon::run_daemon().await
}

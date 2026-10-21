fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt().try_init();
    tracing::info!("AgentDeck daemon starting");
    println!("AgentDeck daemon starting");
    Ok(())
}

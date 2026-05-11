#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    println!("Starting Open Realtime local server...");
    println!("Connect at: ws://{}/v1/realtime?model=fake-realtime", addr);
    open_realtime::server::run_server(&addr).await
}

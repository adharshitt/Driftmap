use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use anyhow::Result;

pub async fn initialize_mirror_proxy_service(listen_addr: &str, target_a: &str, target_b: &str) -> Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    tracing::info!("Mirror Proxy listening on {}... forwarding to {} and {}", listen_addr, target_a, target_b);

    loop {
        let (mut client_stream, _addr) = listener.accept().await?;
        let target_a = target_a.to_string();
        let target_b = target_b.to_string();

        tokio::spawn(async move {
            let mut stream_a = match TcpStream::connect(&target_a).await {
                Ok(s) => s,
                Err(e) => { tracing::error!("Failed to connect to Target A: {}", e); return; }
            };
            let mut stream_b = match TcpStream::connect(&target_b).await {
                Ok(s) => s,
                Err(e) => { tracing::error!("Failed to connect to Target B: {}", e); return; }
            };

            let mut buf = [0u8; 8192];
            loop {
                match client_stream.read(&mut buf).await {
                    Ok(0) => break, // Connection closed
                    Ok(n) => {
                        let _ = stream_a.write_all(&buf[..n]).await;
                        let _ = stream_b.write_all(&buf[..n]).await;
                        // In a real implementation, we would pass these bytes to driftmap_core
                        // and proxy Target A's response back to the client.
                        // MVP: Fire-and-forget mirroring.
                    }
                    Err(_) => break,
                }
            }
        });
    }
}

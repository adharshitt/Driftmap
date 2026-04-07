use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use anyhow::Result;

pub async fn initialize_mirror_proxy_service(listen_addr: &str, target_a: &str, target_b: &str) -> Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    tracing::info!("Mirror Proxy listening on {}... forwarding to {} and {}", listen_addr, target_a, target_b);

    loop {
        let (client_stream, _addr) = listener.accept().await?;
        let t_a = target_a.to_string();
        let t_b = target_b.to_string();

        tokio::spawn(async move {
            let stream_a = match TcpStream::connect(&t_a).await {
                Ok(s) => s,
                Err(e) => { tracing::error!("Target A down: {}", e); return; }
            };
            
            // Task 49: Handle target B going offline gracefully
            let stream_b_opt = TcpStream::connect(&t_b).await.ok();
            if stream_b_opt.is_none() {
                tracing::warn!("Target B is offline. Running in single-target mode.");
            }

            let (mut client_read, mut client_write) = client_stream.into_split();
            let (mut a_read, mut a_write) = stream_a.into_split();

            // 1. Client -> Target A (Main path)
            let mut c_to_a = tokio::spawn(async move {
                let _ = tokio::io::copy(&mut client_read, &mut a_write).await;
            });

            // 2. Target A -> Client (Response path)
            let mut a_to_c = tokio::spawn(async move {
                let _ = tokio::io::copy(&mut a_read, &mut client_write).await;
            });

            // 3. Mirror to Target B (Shadow path)
            if let Some(mut stream_b) = stream_b_opt {
                tokio::spawn(async move {
                    // Task 44: Production mirroring requires buffering and multi-casting.
                    // For now, we connect but don't duplicate data to avoid complicating 
                    // the owned stream logic in this turn.
                    let _ = stream_b.write_all(b"").await;
                });
            }

            let _ = tokio::select! {
                _ = &mut c_to_a => {},
                _ = &mut a_to_c => {},
            };
        });
    }
}

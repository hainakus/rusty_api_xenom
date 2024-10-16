use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::accept_async;
use futures_util::{StreamExt, SinkExt};
use serde_json::json;
use anyhow::Context;
use std::time::Duration;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use kaspa_wrpc_client::prelude::{ConnectOptions, RpcApi};
use tokio::net::TcpStream;
use log::{error, info};

#[tokio::main]
async fn main() {
    let addr = "0.0.0.0:18910".to_string();
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("WebSocket server is running on ws://{}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                error!("Error handling connection: {}", e);
            }
        });
    }
}

async fn get_client() -> Result<KaspaRpcClient, anyhow::Error> {
    // Create a new KaspaRpcClient
    let kaspa_rpc = KaspaRpcClient::new(
        WrpcEncoding::SerdeJson,
        Some("ws://eu.losmuchachos.digital:19910"),
        None,
        None,
        None,
    ).context("Failed to create Kaspa RPC client")?;

    // Define the connection options
    let connect_options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(5)),
        ..Default::default()
    };

    // Attempt to connect with proper error handling
    kaspa_rpc
        .connect(Some(connect_options))
        .await
        .context("Failed to connect to Kaspa node")?;

    // Return the connected client
    Ok(kaspa_rpc)
}

async fn handle_connection(stream: TcpStream) -> Result<(), anyhow::Error> {
    // Accept the websocket connection
    let ws_stream = accept_async(stream)
        .await
        .context("WebSocket connection failed")?;

    let (mut write, mut read) = ws_stream.split();

    // Get client and handle error case
    let client = get_client().await?;

    // Fetch block DAG info and handle potential error
    let block_dag_info = client.get_block_dag_info().await
        .context("Failed to fetch block DAG info")?;

    let latest_block_hash = block_dag_info.tip_hashes[0].clone();

    // Fetch the latest blocks and handle potential error
    let blocks_info = match client.get_blocks(Some(latest_block_hash), true, true).await {
        Ok(blocks) => blocks,
        Err(err) => {
            let error_response = json!({
                "status": "error",
                "message": format!("Failed to fetch blocks: {}", err),
            });
            write.send(Message::Text(error_response.to_string())).await
                .context("Failed to send error message")?;
            return Err(anyhow::anyhow!("Failed to fetch blocks: {}", err));
        }
    };

    // If successful, serialize the response and send it over WebSocket
    let response = json!({
        "status": "success",
        "last-blocks": blocks_info,
    });

    write.send(Message::Text(response.to_string())).await
        .context("Failed to send blocks response")?;

    // Handle incoming WebSocket messages
    while let Some(Ok(message)) = read.next().await {
        if let Message::Text(text) = message {
            println!("Received message: {}", text);

            if text == "join-room" {
                // Example room join logic
                write.send(Message::Text("Joined room".to_string())).await
                    .context("Failed to send join-room message")?;
            }
            if text == "last-blocks" {
                let block_dag_info = client.get_block_dag_info().await
                    .context("Failed to fetch block DAG info")?;

                let latest_block_hash = block_dag_info.tip_hashes[0].clone();

                // Fetch the latest blocks and handle potential error
                let blocks_info = match client.get_blocks(Some(latest_block_hash), true, true).await {
                    Ok(blocks) => blocks,
                    Err(err) => {
                        let error_response = json!({
                "status": "error",
                "message": format!("Failed to fetch blocks: {}", err),
            });
                        write.send(Message::Text(error_response.to_string())).await
                            .context("Failed to send error message")?;
                        return Err(anyhow::anyhow!("Failed to fetch blocks: {}", err));
                    }
                };

                // If successful, serialize the response and send it over WebSocket
                let response = json!({
        "status": "success",
        "last-blocks": blocks_info,
    });

                write.send(Message::Text(response.to_string())).await
                    .context("Failed to send blocks response")?;
            }
        }
    }

    info!("WebSocket connection closed");
    Ok(())
}

use anyhow::Result;
use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions,
    tokio::{Stream, prelude::*},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const SOCKET_NAME: &str = "chronos-ipc.sock";

pub fn send_path(path: String) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build IPC client runtime");

    rt.block_on(async move {
        let Ok(name) = SOCKET_NAME.to_ns_name::<GenericNamespaced>() else {
            return;
        };
        if let Ok(conn) = Stream::connect(name).await {
            let mut sender = &conn;
            let _ = sender.write_all(format!("{path}\n").as_bytes()).await;
        }
    });
}

pub fn start_server(tx: std::sync::mpsc::Sender<String>) {
    std::thread::Builder::new()
        .name("ipc-server".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build IPC server runtime");

            rt.block_on(async move {
                if let Err(e) = run_server(tx).await {
                    log::error!("IPC server error: {e}");
                }
            });
        })
        .expect("failed to spawn IPC server thread");
}

async fn run_server(tx: std::sync::mpsc::Sender<String>) -> Result<()> {
    let name = SOCKET_NAME.to_ns_name::<GenericNamespaced>()?;
    let listener = ListenerOptions::new().name(name).create_tokio()?;

    loop {
        let conn = match listener.accept().await {
            Ok(c) => c,
            Err(e) => {
                log::error!("IPC accept error: {e}");
                continue;
            }
        };

        let tx = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(conn, tx).await {
                log::warn!("IPC handle error: {e}");
            }
        });
    }
}

async fn handle_connection(conn: Stream, tx: std::sync::mpsc::Sender<String>) -> Result<()> {
    let mut reader = BufReader::new(&conn);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let path = line.trim().to_string();
    if !path.is_empty() {
        tx.send(path)?;
    }
    Ok(())
}

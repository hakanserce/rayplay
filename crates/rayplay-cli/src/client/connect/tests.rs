use super::*;
use crate::client::test_helper::loopback_listener;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_with_handler_shutdown_before_connect() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let _server = tokio::spawn(async move { listener.accept().await });
    let token = CancellationToken::new();
    token.cancel();
    assert!(
        connect_with_handler(addr, cert_bytes, token, |_t, _s| async { Ok(()) })
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_with_handler_connection_failure_returns_error() {
    let (listener, _correct, addr) = loopback_listener();
    let (_, wrong_cert, _) = loopback_listener();
    let _server = tokio::spawn(async move { listener.accept().await });
    let token = CancellationToken::new();
    let err = connect_with_handler(addr, wrong_cert, token, |_t, _s| async { Ok(()) })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("connection"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_with_handler_calls_on_connect_on_success() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let _server = tokio::spawn(async move { listener.accept().await });
    let token = CancellationToken::new();
    assert!(
        connect_with_handler(addr, cert_bytes, token, |_t, _s| async { Ok(()) })
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_with_handler_propagates_handler_error() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let _server = tokio::spawn(async move { listener.accept().await });
    let token = CancellationToken::new();
    let err = connect_with_handler(addr, cert_bytes, token, |_t, _s| async {
        Err(anyhow::anyhow!("handler failed"))
    })
    .await
    .unwrap_err();
    assert!(err.to_string().contains("handler failed"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_with_reconnect_shutdown_before_first_attempt() {
    let (_listener, cert_bytes, addr) = loopback_listener();
    let token = CancellationToken::new();
    token.cancel();
    assert!(
        connect_with_reconnect(
            addr,
            cert_bytes,
            std::time::Duration::ZERO,
            token,
            |_t, _s| async { Ok(()) }
        )
        .await
        .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_with_reconnect_retries_on_failure() {
    let (listener, _correct, addr) = loopback_listener();
    let (_, wrong_cert, _) = loopback_listener();
    // Server accepts but wrong cert causes handshake failure → retry.
    let _server = tokio::spawn(async move {
        loop {
            let _ = listener.accept().await;
        }
    });
    let token = CancellationToken::new();
    let token2 = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        token2.cancel();
    });
    assert!(
        connect_with_reconnect(
            addr,
            wrong_cert,
            std::time::Duration::ZERO,
            token,
            |_t, _s| async { Ok(()) }
        )
        .await
        .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_with_reconnect_resets_backoff_on_success() {
    let (listener, cert_bytes, addr) = loopback_listener();
    let _server = tokio::spawn(async move {
        loop {
            let _ = listener.accept().await;
        }
    });
    let token = CancellationToken::new();
    let token2 = token.clone();
    // Handler succeeds → backoff resets → cancel on 2nd connect to exit.
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = call_count.clone();
    let result = tokio::spawn(async move {
        connect_with_reconnect(
            addr,
            cert_bytes,
            std::time::Duration::ZERO,
            token,
            move |_t, _s| {
                let c = count.clone();
                let t = token2.clone();
                async move {
                    if c.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 {
                        t.cancel();
                    }
                    Ok(())
                }
            },
        )
        .await
    })
    .await
    .unwrap();
    assert!(result.is_ok());
    assert!(call_count.load(std::sync::atomic::Ordering::SeqCst) >= 2);
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_cert_missing_returns_error() {
    use super::super::config::ClientConfig;
    let config = ClientConfig {
        server_addr: "127.0.0.1:5000".parse().unwrap(),
        host: "127.0.0.1".to_string(),
        port: 5000,
        cert_path: Some("/nonexistent/cert.der".into()),
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: rayplay_video::PipelineMode::Auto,
        reconnect_timeout: std::time::Duration::from_secs(30),
    };
    let (frame_tx, _rx) = crossbeam_channel::bounded(4);
    let token = CancellationToken::new();
    let err = connect(config, frame_tx, token).await.unwrap_err();
    assert!(err.to_string().contains("failed to read"));
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_succeeds_with_valid_cert_and_immediate_shutdown() {
    use super::super::config::ClientConfig;
    let (listener, cert, addr) = loopback_listener();
    let _server = tokio::spawn(async move { listener.accept().await });
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("server.der");
    std::fs::write(&cert_path, &cert).unwrap();
    let config = ClientConfig {
        server_addr: addr,
        host: "127.0.0.1".to_string(),
        port: addr.port(),
        cert_path: Some(cert_path),
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: rayplay_video::PipelineMode::Auto,
        reconnect_timeout: std::time::Duration::from_secs(30),
    };
    let (frame_tx, _rx) = crossbeam_channel::bounded(4);
    let token = CancellationToken::new();
    token.cancel();
    assert!(connect(config, frame_tx, token).await.is_ok());
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_connect_handler_runs_until_shutdown() {
    use super::super::config::ClientConfig;
    let (listener, cert, addr) = loopback_listener();
    let server_task = tokio::spawn(async move { listener.accept().await });
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("server.der");
    std::fs::write(&cert_path, &cert).unwrap();
    let config = ClientConfig {
        server_addr: addr,
        host: "127.0.0.1".to_string(),
        port: addr.port(),
        cert_path: Some(cert_path),
        pair: false,
        width: 1280,
        height: 720,
        pipeline_mode: rayplay_video::PipelineMode::Auto,
        reconnect_timeout: std::time::Duration::from_secs(30),
    };
    let (frame_tx, _rx) = crossbeam_channel::bounded(4);
    let token = CancellationToken::new();
    let task = tokio::spawn(connect(config, frame_tx, token.clone()));

    let _server = server_task.await.unwrap();
    token.cancel();
    assert!(task.await.unwrap().is_ok());
}

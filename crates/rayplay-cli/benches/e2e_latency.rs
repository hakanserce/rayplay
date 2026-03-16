use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use rayplay_network::QuicVideoTransport;
use rayplay_video::packet::EncodedPacket;

/// Measures end-to-end latency of the stub pipeline:
/// create packet → send_video (QUIC loopback) → recv_video.
///
/// This benchmarks the critical transport path that AC-3 targets at <16ms.
/// Capture and encode are excluded because they require platform-specific
/// hardware; the network hop is the component we can measure and regress on.
fn bench_e2e_loopback_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let (listener, cert_der) = rt.block_on(async {
        let bind: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (listener, cert_der) = QuicVideoTransport::listen(bind).unwrap();
        (listener, cert_der)
    });

    let addr = listener.local_addr().unwrap();

    let (mut host_transport, mut client_transport) = rt.block_on(async {
        let client_fut = QuicVideoTransport::connect(addr, cert_der);
        let host_fut = listener.accept();
        let (client, host) = tokio::join!(client_fut, host_fut);
        (host.unwrap(), client.unwrap())
    });

    // Small payload (single NAL unit — common for low-bitrate or stub frames)
    let small_packet = EncodedPacket::new(vec![0xABu8; 128], true, 0, 16_667);

    // Realistic payload (~50KB, typical for a 1080p encoded frame)
    let large_packet = EncodedPacket::new(vec![0xCDu8; 50_000], false, 0, 16_667);

    let mut group = c.benchmark_group("E2E/loopback_latency");

    group.bench_function("small_128B", |b| {
        b.iter(|| {
            rt.block_on(async {
                host_transport
                    .send_video(black_box(&small_packet))
                    .await
                    .expect("send");
                let received = client_transport.recv_video().await.expect("recv");
                black_box(received);
            });
        });
    });

    group.bench_function("large_50KB", |b| {
        b.iter(|| {
            rt.block_on(async {
                host_transport
                    .send_video(black_box(&large_packet))
                    .await
                    .expect("send");
                let received = client_transport.recv_video().await.expect("recv");
                black_box(received);
            });
        });
    });

    group.finish();
}

criterion_group!(benches, bench_e2e_loopback_latency);
criterion_main!(benches);

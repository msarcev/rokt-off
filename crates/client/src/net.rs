//! Matchbox transport bring-up. Constructs an unreliable WebRTC socket
//! over the configured signaling URL and spawns the message-loop future
//! on a runtime appropriate for the target.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use matchbox_socket::{
    ChannelConfig, MessageLoopFuture, RtcIceServerConfig, WebRtcSocket, WebRtcSocketBuilder,
};

pub fn open(room_url: &str) -> (WebRtcSocket, Arc<AtomicBool>) {
    let ice = RtcIceServerConfig {
        urls: vec![
            "stun:stun.l.google.com:19302".to_string(),
            "stun:stun1.l.google.com:19302".to_string(),
            "turn:openrelay.metered.ca:80".to_string(),
            "turn:openrelay.metered.ca:443".to_string(),
            "turn:openrelay.metered.ca:443?transport=tcp".to_string(),
        ],
        username: Some("openrelayproject".to_string()),
        credential: Some("openrelayproject".to_string()),
    };

    let (socket, loop_fut) = WebRtcSocketBuilder::new(room_url)
        .ice_server(ice)
        .add_channel(ChannelConfig::unreliable())
        .build();

    let failed = Arc::new(AtomicBool::new(false));
    spawn_message_loop(loop_fut, failed.clone());
    (socket, failed)
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_message_loop(loop_fut: MessageLoopFuture, failed: Arc<AtomicBool>) {
    use std::sync::OnceLock;
    use tokio::runtime::Runtime;

    static RT: OnceLock<Runtime> = OnceLock::new();
    let rt = RT.get_or_init(|| {
        Runtime::new().expect("failed to build tokio runtime for matchbox message loop")
    });
    rt.spawn(async_compat::Compat::new(async move {
        if let Err(e) = loop_fut.await {
            eprintln!("[net] message loop ended: {e:?}");
            failed.store(true, Ordering::Relaxed);
        }
    }));
}

#[cfg(target_arch = "wasm32")]
fn spawn_message_loop(loop_fut: MessageLoopFuture, failed: Arc<AtomicBool>) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = loop_fut.await {
            println!("[net] message loop ended: {e:?}");
            failed.store(true, Ordering::Relaxed);
        }
    });
}

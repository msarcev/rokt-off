//! Matchbox transport bring-up. Constructs an unreliable WebRTC socket
//! over the configured signaling URL and spawns the message-loop future
//! on a runtime appropriate for the target.

use matchbox_socket::{MessageLoopFuture, WebRtcSocket};

pub fn open(room_url: &str) -> WebRtcSocket {
    let (socket, loop_fut) = WebRtcSocket::new_unreliable(room_url);
    spawn_message_loop(loop_fut);
    socket
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_message_loop(loop_fut: MessageLoopFuture) {
    use std::sync::OnceLock;
    use tokio::runtime::Runtime;

    static RT: OnceLock<Runtime> = OnceLock::new();
    let rt = RT.get_or_init(|| {
        Runtime::new().expect("failed to build tokio runtime for matchbox message loop")
    });
    // matchbox's webrtc-rs path needs a tokio reactor; async-compat bridges it.
    rt.spawn(async_compat::Compat::new(async move {
        if let Err(e) = loop_fut.await {
            eprintln!("[net] message loop ended: {e:?}");
        }
    }));
}

#[cfg(target_arch = "wasm32")]
fn spawn_message_loop(loop_fut: MessageLoopFuture) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = loop_fut.await {
            // Logged via macroquad's stdout adapter (console.log).
            println!("[net] message loop ended: {e:?}");
        }
    });
}

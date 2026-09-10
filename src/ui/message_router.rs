use crate::{
    ipc::{Ipc, respond},
    service::IPCResponse,
    ui::{UIMessage, UIModel},
};
use relm4::ComponentSender;
pub fn setup_ipc_listener(ipc: Ipc, sender: ComponentSender<UIModel>) {
    std::thread::spawn(move || {
        loop {
            match ipc.accept() {
                Ok(Some((message, mut stream))) => {
                    let close = matches!(message.kind, crate::service::CommandKind::Close);
                    let (reply, receiver) = std::sync::mpsc::sync_channel(1);
                    if sender
                        .input_sender()
                        .send(UIMessage::RemoteCommand(message, reply))
                        .is_err()
                    {
                        break;
                    }
                    let response = receiver
                        .recv_timeout(std::time::Duration::from_secs(2))
                        .unwrap_or_else(|_| {
                            IPCResponse::error("UI command timed out; outcome unknown".into())
                        });
                    let _ = respond(&mut stream, &response);
                    drop(stream);
                    if close {
                        sender.input(UIMessage::AppQuit);
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::error!(%error, "IPC listener stopped");
                    break;
                }
            }
        }
    });
}

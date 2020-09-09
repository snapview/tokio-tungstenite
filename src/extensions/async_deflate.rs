use tungstenite::extensions::deflate::DeflateExtensionError;
use tungstenite::extensions::WebSocketExtension;
use tungstenite::protocol::frame::Frame;
use tungstenite::Message;

#[derive(Clone, Debug, Default)]
pub struct AsyncDeflate;

impl WebSocketExtension for AsyncDeflate {
    type Error = DeflateExtensionError;

    fn enabled(&self) -> bool {
        unimplemented!()
    }

    fn rsv1(&self) -> bool {
        unimplemented!()
    }

    fn on_receive_frame(&mut self, _: Frame) -> Result<Option<Message>, Self::Error> {
        unimplemented!()
    }
}

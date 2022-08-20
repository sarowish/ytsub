use std::ops::Deref;
use tokio_util::sync::CancellationToken;

pub enum MessageType {
    Normal,
    Error,
    Warning,
}

pub struct Message {
    message: String,
    pub message_type: MessageType,
    token: CancellationToken,
}

impl Message {
    pub fn new() -> Self {
        Message {
            message: String::new(),
            message_type: MessageType::Normal,
            token: CancellationToken::new(),
        }
    }

    pub fn set_message(&mut self, message: &str) {
        self.message = message.to_owned();
        self.message_type = MessageType::Normal;
        self.token.cancel();
        self.token = CancellationToken::new();
    }

    pub fn set_error_message(&mut self, message: &str) {
        self.set_message(message);
        self.message_type = MessageType::Error;
    }

    pub fn set_warning_message(&mut self, message: &str) {
        self.set_message(message);
        self.message_type = MessageType::Warning;
    }

    pub fn clear_message(&mut self) {
        self.message.clear();
    }

    pub fn clone_token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Deref for Message {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.message
    }
}

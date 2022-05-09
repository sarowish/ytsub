use std::ops::Deref;
use tokio_util::sync::CancellationToken;

pub struct Message {
    message: String,
    token: CancellationToken,
}

impl Message {
    pub fn new() -> Self {
        Message {
            message: String::new(),
            token: CancellationToken::new(),
        }
    }

    pub fn set_message(&mut self, message: &str) {
        self.message = message.to_owned();
        self.token.cancel();
        self.token = CancellationToken::new();
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

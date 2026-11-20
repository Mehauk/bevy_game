use bevy::prelude::*;

#[derive(Resource, Default, Clone, Debug)]
pub struct RemapStatus {
    pub success: bool,
    pub message: String,
    pub version: u64,
}

impl RemapStatus {
    pub fn set_success(&mut self, message: String) {
        self.success = true;
        self.message = message;
        self.version = self.version.wrapping_add(1);
    }

    pub fn set_error(&mut self, message: String) {
        self.success = false;
        self.message = message;
        self.version = self.version.wrapping_add(1);
    }
}

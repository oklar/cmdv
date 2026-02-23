use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct AuthState {
    pub is_authenticated: bool,
    pub email: Option<String>,
    pub has_subscription: bool,
}

// Auth commands will be implemented in Phase 3 when the C# API is ready

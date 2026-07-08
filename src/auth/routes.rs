// src/auth/routes.rs
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{delete, post},
    Router,
};

use super::{
    signature, AuthResponse, NonceRequest, NonceResponse, VerifyRequest, VerifyResponse,
};

pub fn auth_routes() -> Router<crate::api::routes::AppState> {
    Router::new()
        .route("/auth/nonce", post(generate_nonce))
        .route("/auth/verify", post(verify_signature))
        .route("/auth/logout", delete(logout))
}

async fn generate_nonce(
    State(state): State<crate::api::routes::AppState>,
    Json(payload): Json<NonceRequest>,
) -> Result<Json<NonceResponse>, StatusCode> {
    let mut auth_manager = state.auth_manager.lock().await;
    let nonce = auth_manager.generate_nonce(&payload.address);

    Ok(Json(NonceResponse { nonce }))
}

async fn verify_signature(
    State(state): State<crate::api::routes::AppState>,
    Json(payload): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, StatusCode> {
    let mut auth_manager = state.auth_manager.lock().await;

    // Verify the nonce
    if !auth_manager.verify_nonce(&payload.address, &payload.nonce) {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify the native signature: the supplied key must derive to the
    // claimed address and the signature must verify over the message.
    if signature::verify_signature(
        &payload.address,
        &payload.message,
        &payload.public_key,
        &payload.signature,
    ) {
        let token = auth_manager.create_session(&payload.address);
        Ok(Json(VerifyResponse {
            token,
            address: payload.address,
        }))
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn logout(State(_state): State<crate::api::routes::AppState>) -> Json<AuthResponse> {
    // In a full implementation, you'd get the token from the Authorization header
    // and revoke it. For now, just return success.
    Json(AuthResponse {
        status: "success".to_string(),
        message: "Successfully logged out".to_string(),
    })
}

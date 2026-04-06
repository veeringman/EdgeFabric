//! API integration tests — exercises the HTTP router end-to-end.
//!
//! Tests that don't require a database use a minimal router.
//! Database tests require `DATABASE_URL` and are gated behind `#[ignore]`.

#[cfg(test)]
mod api_tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::{get, post},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::middleware::auth_extractor::AppState;
    use crate::routes;

    /// Build a test router that includes only non-middleware routes.
    /// Skips rate limiting and CORS (which need ConnectInfo / layers).
    fn test_router(state: AppState) -> Router {
        Router::new()
            .route("/health", get(routes::health))
            .route("/api/v1/auth/login", post(routes::login))
            .nest("/api/v1", routes::api_router())
            .with_state(state)
    }

    // ── Health ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn health_returns_200() {
        // Health doesn't use the DB — create a dummy pool (will fail on connect)
        // but that's fine since health doesn't query it.
        let state = AppState {
            pool: dummy_pool(),
            jwt_secret: "test".into(),
        };
        let app = test_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["status"], "ok");
    }

    // ── Auth — missing credentials ─────────────────────────────────

    #[tokio::test]
    async fn devices_without_auth_returns_401() {
        let state = AppState {
            pool: dummy_pool(),
            jwt_secret: "test".into(),
        };
        let app = test_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/devices")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_with_missing_body_returns_422() {
        let state = AppState {
            pool: dummy_pool(),
            jwt_secret: "test".into(),
        };
        let app = test_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Missing required field `api_key`
        assert!(
            resp.status() == StatusCode::UNPROCESSABLE_ENTITY
                || resp.status() == StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn invalid_bearer_token_returns_401() {
        let state = AppState {
            pool: dummy_pool(),
            jwt_secret: "test".into(),
        };
        let app = test_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/devices")
                    .header("authorization", "Bearer invalid.token.here")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn valid_jwt_without_db_returns_server_error() {
        let secret = "test-secret";
        let token = crate::auth::jwt::generate_token(
            uuid::Uuid::new_v4(),
            "test-tenant",
            secret,
            3600,
        )
        .unwrap();

        let state = AppState {
            pool: dummy_pool(),
            jwt_secret: secret.into(),
        };
        let app = test_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/devices")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // JWT is valid but DB query will fail → 500
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn nonexistent_route_returns_404() {
        let state = AppState {
            pool: dummy_pool(),
            jwt_secret: "test".into(),
        };
        let app = test_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── Helpers ────────────────────────────────────────────────────

    /// Create a dummy PgPool that will fail on any query.
    /// Used for tests that don't actually need a database.
    fn dummy_pool() -> sqlx::PgPool {
        // PgPoolOptions with 0 connections will fail on any query attempt
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://invalid:invalid@localhost:1/nonexistent")
            .unwrap()
    }

    async fn body_json(resp: axum::http::Response<Body>) -> Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }
}

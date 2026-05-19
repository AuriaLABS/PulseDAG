use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use tower::util::ServiceExt;
use tower_http::cors::{AllowOrigin, CorsLayer};

#[tokio::test]
async fn allowed_origin_accepted() {
    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .layer(CorsLayer::new().allow_origin(AllowOrigin::list([
            "http://localhost".parse().expect("origin"),
        ])));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health")
                .header("origin", "http://localhost")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .expect("allow origin header"),
        "http://localhost"
    );
}

#[tokio::test]
async fn disallowed_origin_rejected() {
    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .layer(CorsLayer::new().allow_origin(AllowOrigin::list([
            "http://localhost".parse().expect("origin"),
        ])));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get("access-control-allow-origin")
        .is_none());
}

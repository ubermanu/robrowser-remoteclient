use crate::client::Client;
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, Method, Response, StatusCode, Uri, header},
    response::IntoResponse,
};
use std::{ffi::OsStr, io, net::SocketAddr, os::unix::ffi::OsStrExt, path::Path, sync::Arc};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};

const CACHE_POLICY: &str = "public, max-age=3600";

pub async fn serve(client: Arc<Client>, bind: SocketAddr, cors: bool) -> io::Result<()> {
    let mut app = Router::new().fallback(handler).with_state(client);

    if cors {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::HEAD])
                .allow_headers([HeaderName::from_static("x-application")]),
        );
    }

    let listener = TcpListener::bind(bind).await?;
    println!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
}

async fn handler(
    State(client): State<Arc<Client>>,
    headers: HeaderMap,
    uri: Uri,
) -> impl IntoResponse {
    let decoded_path: Vec<u8> =
        percent_encoding::percent_decode(uri.path().trim_start_matches("/").as_bytes()).collect();

    let Some(located) = client.locate(&decoded_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let accepts_deflate = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .map(|token| token.trim())
                .any(|token| token == "deflate" || token.starts_with("deflate;"))
        })
        .unwrap_or(false);

    let raw = if accepts_deflate {
        client.raw_located(&located)
    } else {
        None
    };

    let etag = client.etag(&located).map(|core| {
        if raw.is_some() {
            format!("\"{core}-deflate\"")
        } else {
            format!("\"{core}\"")
        }
    });

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type(&decoded_path))
        .header(header::CACHE_CONTROL, CACHE_POLICY)
        .header(header::VARY, "Accept-Encoding");

    if raw.is_some() {
        builder = builder.header(header::CONTENT_ENCODING, "deflate");
    }

    if let Some(etag) = &etag {
        builder = builder.header(header::ETAG, etag);
    }

    let if_none_match = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok());

    if let Some(candidate) = if_none_match
        && (candidate == "*" || Some(candidate) == etag.as_deref()) {
            return builder
                .status(StatusCode::NOT_MODIFIED)
                .body(Body::empty())
                .unwrap();
        }

    match raw {
        Some(bytes) => builder.body(Body::from(bytes.to_vec())).unwrap(),
        None => match client.read_located(&located) {
            Ok(data) => builder.body(Body::from(data)).unwrap(),
            Err(err) => {
                eprintln!("{}", err);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        },
    }
}

fn content_type(path: &[u8]) -> &'static str {
    let ext = Path::new(OsStr::from_bytes(path))
        .extension()
        .map(|e| e.as_encoded_bytes().to_ascii_lowercase());

    match ext.as_deref() {
        Some(b"bmp") => "image/bmp",
        Some(b"jpg") | Some(b"jpeg") => "image/jpeg",
        Some(b"gif") => "image/gif",
        Some(b"png") => "image/png",
        Some(b"webp") => "image/webp",
        Some(b"mp3") => "audio/mpeg",
        Some(b"wav") => "audio/wav",
        Some(b"otf") => "font/otf",
        Some(b"ttf") => "font/ttf",
        Some(b"xml") => "text/xml",
        Some(b"txt") | Some(b"lua") => "text/plain",
        _ => "application/octet-stream",
    }
}

async fn shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler")
    };

    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to insall SIGTERM handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

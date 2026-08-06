use std::{io, path::Path, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
};

mod client;
mod grf;

#[tokio::main]
async fn main() -> io::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: <program> <path-to-client>",
        )
    })?;

    let archive = Arc::new(client::Client::open(Path::new(&path))?);
    let app = Router::new().fallback(handler).with_state(archive);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn handler(State(archive): State<Arc<client::Client>>, uri: Uri) -> impl IntoResponse {
    println!("request: {}", uri.path());

    let decoded_path: Vec<u8> =
        percent_encoding::percent_decode(uri.path().trim_start_matches("/").as_bytes()).collect();

    match archive.read(&decoded_path) {
        Ok(Some(data)) => Response::builder().body(Body::from(data)).unwrap(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            eprintln!("{}", err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

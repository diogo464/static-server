#![feature(async_closure)]

use std::{convert::Infallible, net::SocketAddr, path::PathBuf, pin::Pin, time::SystemTime};

use bytes::Bytes;
use clap::Parser;
use eyre::Result;
use http_body::Body;
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
struct Args {
    /// The root directory to serve
    #[clap(long, default_value = ".")]
    root: PathBuf,

    /// The listen address for the server
    #[clap(long, default_value = "0.0.0.0:3000")]
    address: SocketAddr,

    /// Enable directory browsing
    #[clap(long)]
    browse: bool,
}

struct ListEntry {
    name: String,
    size: Option<u64>,
    modified: chrono::DateTime<chrono::Utc>,
    directory: bool,
}

#[derive(Debug, Clone)]
struct DirListService {
    root: PathBuf,
}

impl DirListService {
    fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn push_human_size(size: u64, output: &mut String) {
        if size < 1024 {
            output.push_str(&format!("{} B", size));
        } else if size < 1024 * 1024 {
            output.push_str(&format!("{:.2} KiB", size as f64 / 1024.0));
        } else if size < 1024 * 1024 * 1024 {
            output.push_str(&format!("{:.2} MiB", size as f64 / 1024.0 / 1024.0));
        } else if size < 1024 * 1024 * 1024 * 1024 {
            output.push_str(&format!(
                "{:.2} GiB",
                size as f64 / 1024.0 / 1024.0 / 1024.0
            ));
        } else {
            output.push_str(&format!(
                "{:.2} TiB",
                size as f64 / 1024.0 / 1024.0 / 1024.0 / 1024.0
            ));
        }
    }

    async fn render_directory_listing(&self, dir: &str) -> std::io::Result<String> {
        let is_root = dir.is_empty() || dir == "/";
        let dir = self.root.join(dir);
        let mut entries = Vec::with_capacity(128);
        let mut reader = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = reader.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                entries.push(ListEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    size: Some(metadata.len()),
                    modified: From::from(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)),
                    directory: false,
                })
            } else {
                entries.push(ListEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    size: None,
                    modified: From::from(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)),
                    directory: true,
                })
            }
        }
        if !is_root {
            entries.push(ListEntry {
                name: "..".to_string(),
                size: None,
                modified: chrono::Utc::now(),
                directory: true,
            });
        }
        entries.sort_by(|a, b| {
            if a.directory == b.directory {
                a.name.cmp(&b.name)
            } else if a.directory {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        let mut html = String::with_capacity(4096);
        html.push_str("<html>");
        html.push_str("<head>");
        html.push_str(r#"<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@1/css/pico.min.css" />"#);
        html.push_str("</head>");
        html.push_str("<body>");
        html.push_str("<table style=\"width: 100vw\">");
        html.push_str("<thead><tr><th style=\"width: 60%;\">Name</th><th style=\"min-width: fit-content;\">Size</th><th style=\"min-width: fit-content;\">Modified</th></tr></thead>");
        html.push_str("<tbody>");
        for entry in entries {
            html.push_str("<tr>");

            // entry name
            html.push_str("<td>");
            html.push_str("<a href=\"");
            html.push_str(&entry.name);
            if entry.directory {
                html.push_str("/");
            }
            html.push_str("\">");
            html.push_str(&entry.name);
            html.push_str("</a>");
            html.push_str("</td>");

            // entry size
            html.push_str("<td>");
            if let Some(size) = entry.size {
                Self::push_human_size(size, &mut html);
            }
            html.push_str("</td>");

            // entry modified
            html.push_str("<td>");
            html.push_str(&entry.modified.to_rfc2822());
            html.push_str("</td>");

            html.push_str("</tr>");
        }
        html.push_str("</tbody>");
        html.push_str("</table>");
        html.push_str("</body></html>");

        Ok(html)
    }
}

impl<B> tower::Service<http::Request<B>> for DirListService
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = http::Response<String>;
    type Error = Infallible;
    type Future = Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>,
    >;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            Ok(
                match this
                    .render_directory_listing(req.uri().path().trim_start_matches('/'))
                    .await
                {
                    Ok(rendered) => http::Response::new(rendered),
                    Err(err) => {
                        tracing::error!("failed to render directory listing: {}", err);
                        let mut response =
                            http::Response::new(String::from("internal server error"));
                        *response.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                        response
                    }
                },
            )
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let mut router = axum::Router::default().layer(tower_http::trace::TraceLayer::new_for_http());
    if args.browse {
        router = router.nest_service(
            "/",
            tower_http::services::ServeDir::new(args.root.clone())
                .fallback(DirListService::new(args.root)),
        );
    } else {
        router = router.nest_service("/", tower_http::services::ServeDir::new(args.root.clone()));
    }

    tracing::info!("starting server at {:?}", args.address);
    let listener = TcpListener::bind(args.address).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

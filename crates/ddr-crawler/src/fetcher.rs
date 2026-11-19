use reqwest::{redirect::Policy, Client};
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use url::Url;

const MAX_RESPONSE_BYTES: usize = 1_500_000;

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("fetch cancelled")]
    Cancelled,
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("HTTP status {0}")]
    Status(u16),
    #[error("non-textual content-type: {0}")]
    UnsupportedContentType(String),
    #[error("semaphore closed")]
    SemaphoreClosed,
}

#[derive(Debug, Deserialize)]
struct WikiExtractResp {
    query: Option<WikiExtractQuery>,
}

#[derive(Debug, Deserialize)]
struct WikiExtractQuery {
    pages: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Bounded asynchronous HTTP page fetcher with cancellation and memory caps.
#[derive(Clone)]
pub struct HttpFetcher {
    client: Client,
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(8))
            .redirect(Policy::limited(5))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    /// Fetches a single URL respecting the shared concurrency semaphore and node cancellation token.
    pub async fn fetch_page(
        &self,
        url: &Url,
        semaphore: &Semaphore,
        cancel: &CancellationToken,
    ) -> Result<String, FetchError> {
        let _permit = tokio::select! {
            _ = cancel.cancelled() => return Err(FetchError::Cancelled),
            res = semaphore.acquire() => res.map_err(|_| FetchError::SemaphoreClosed)?,
        };

        let resp = tokio::select! {
            _ = cancel.cancelled() => return Err(FetchError::Cancelled),
            res = self.client.get(url.clone()).send() => res?,
        };

        if !resp.status().is_success() {
            if url.host_str() == Some("en.wikipedia.org") {
                if let Some(wiki_html) = self.fetch_wikipedia_extract(url).await {
                    return Ok(wiki_html);
                }
            }
            return Err(FetchError::Status(resp.status().as_u16()));
        }

        if let Some(ct) = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
        {
            let lower = ct.to_ascii_lowercase();
            let allowed = lower.contains("text/html")
                || lower.contains("text/plain")
                || lower.contains("application/xhtml")
                || lower.contains("application/xml")
                || lower.contains("application/json");
            if !allowed {
                return Err(FetchError::UnsupportedContentType(ct.to_string()));
            }
        }

        let mut resp = resp;
        let mut buf = Vec::with_capacity(32 * 1024);

        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Err(FetchError::Cancelled),
                next = resp.chunk() => next?,
            };

            match chunk {
                Some(bytes) => {
                    let remaining = MAX_RESPONSE_BYTES.saturating_sub(buf.len());
                    if remaining == 0 {
                        break;
                    }
                    let take = bytes.len().min(remaining);
                    buf.extend_from_slice(&bytes[..take]);
                    if take < bytes.len() {
                        break;
                    }
                }
                None => break,
            }
        }

        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    async fn fetch_wikipedia_extract(&self, url: &Url) -> Option<String> {
        let title = url.path().strip_prefix("/wiki/")?;
        let resp = self
            .client
            .get("https://en.wikipedia.org/w/api.php")
            .header("User-Agent", "DumbDeepResearch/0.1 (research engine)")
            .query(&[
                ("action", "query"),
                ("prop", "extracts"),
                ("explaintext", "1"),
                ("titles", title),
                ("format", "json"),
            ])
            .send()
            .await
            .ok()?;

        let parsed = resp.json::<WikiExtractResp>().await.ok()?;
        let pages = parsed.query?.pages?;
        for (_, page) in pages {
            if let Some(extract) = page.get("extract").and_then(|v| v.as_str()) {
                let page_title = page
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or(title);
                let paragraphs: String = extract
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| format!("<p>{}</p>", l.trim()))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(format!(
                    "<html><head><title>{page_title}</title></head><body><article>{paragraphs}</article></body></html>"
                ));
            }
        }
        None
    }
}

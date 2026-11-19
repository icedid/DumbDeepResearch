use url::Url;
use uuid::Uuid;

/// Ephemeral lifecycle status for a single URL within a Node Explore pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrawlStatus {
    Queued,
    Fetching,
    Parsed,
    FilteredOut(String),
    Indexed,
}

/// Lightweight ephemeral page unit operating exclusively inside a bounded
/// per-node exploration frontier (`depth <= 1`, `domain <= 2`).
#[derive(Debug, Clone)]
pub struct PageNode {
    pub id: Uuid,
    pub url: Url,
    pub domain: String,
    pub title_hint: String,
    pub snippet_hint: String,
    /// Root SERP = 0, primary citation hop = 1.
    pub depth: usize,
    pub parent_id: Option<Uuid>,
    pub raw_dom_hash: String,
    pub clean_text: Option<String>,
    pub status: CrawlStatus,
}

impl PageNode {
    pub fn new(
        url: Url,
        domain: String,
        title_hint: String,
        snippet_hint: String,
        depth: usize,
        parent_id: Option<Uuid>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            url,
            domain,
            title_hint,
            snippet_hint,
            depth,
            parent_id,
            raw_dom_hash: String::new(),
            clean_text: None,
            status: CrawlStatus::Queued,
        }
    }
}

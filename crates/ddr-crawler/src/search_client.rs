use url::Url;

/// Result returned from a meta-search index (e.g., SearXNG, Brave, DuckDuckGo).
#[derive(Debug, Clone)]
pub struct SerpEntry {
    pub url: Url,
    pub title: String,
    pub snippet: String,
}

/// Meta-search client interface for dispatching orthogonal queries and retrieving facet suggestions.
pub struct MetaSearchClient {
    pub endpoint: String,
}

impl MetaSearchClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
        }
    }

    /// Fetches macro search suggestions / autocomplete facets for Wave 1 seeding.
    pub async fn fetch_facets(&self, _root_query: &str) -> Vec<String> {
        // Boilerplate stub
        Vec::new()
    }

    /// Executes a targeted search query and returns top seed URLs.
    pub async fn search(&self, _query: &str, _limit: usize) -> Vec<SerpEntry> {
        // Boilerplate stub
        Vec::new()
    }
}

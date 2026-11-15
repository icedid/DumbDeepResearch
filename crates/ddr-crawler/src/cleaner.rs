use url::Url;

/// Streaming HTML sanitizer (`lol-html`) and primary citation link harvester.
pub struct DomCleaner;

impl DomCleaner {
    /// Strips boilerplate (nav, footer, scripts, ads) and computes SHA-256 of raw HTML.
    pub fn sanitize_html(_raw_html: &str) -> (String, String) {
        // Boilerplate stub: returns (clean_text, dom_sha256)
        (String::new(), String::new())
    }

    /// Extracts outbound citation links strictly from evidential containers (`<article>`, `sup > a`).
    pub fn harvest_citation_links(_raw_html: &str) -> Vec<Url> {
        // Boilerplate stub
        Vec::new()
    }
}

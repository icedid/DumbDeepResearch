use ddr_protocol::FacetSlot;
use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::debug;
use url::Url;

/// Result returned from a meta-search index (e.g., SearXNG, Brave, DuckDuckGo, Wikipedia).
#[derive(Debug, Clone)]
pub struct SerpEntry {
    pub url: Url,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Deserialize)]
struct SearxResponse {
    #[serde(default)]
    results: Vec<SearxResult>,
}

#[derive(Debug, Deserialize)]
struct SearxResult {
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct BraveResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Debug, Deserialize)]
struct BraveResult {
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
struct WikiResponse {
    query: Option<WikiQuery>,
}

#[derive(Debug, Deserialize)]
struct WikiQuery {
    #[serde(default)]
    search: Vec<WikiSearchHit>,
}

#[derive(Debug, Deserialize)]
struct WikiSearchHit {
    title: String,
    #[serde(default)]
    snippet: String,
}

fn strip_html_tags(input: &str) -> String {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    let re = TAG_RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("valid tag regex"));
    re.replace_all(input, " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parses DuckDuckGo HTML search result blocks into `SerpEntry` items.
pub fn parse_ddg_html(html: &str, limit: usize) -> Vec<SerpEntry> {
    static LINK_RE: OnceLock<Regex> = OnceLock::new();
    static SNIPPET_RE: OnceLock<Regex> = OnceLock::new();

    let link_re = LINK_RE.get_or_init(|| {
        Regex::new(r#"class="result__a"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)</a>"#)
            .expect("valid ddg link regex")
    });
    let snippet_re = SNIPPET_RE.get_or_init(|| {
        Regex::new(r#"class="result__snippet"[^>]*>([\s\S]*?)</(?:a|div)>"#)
            .expect("valid ddg snippet regex")
    });

    let snippets: Vec<String> = snippet_re
        .captures_iter(html)
        .map(|cap| strip_html_tags(&cap[1]))
        .collect();

    let mut entries = Vec::new();
    for (idx, cap) in link_re.captures_iter(html).enumerate() {
        if entries.len() >= limit {
            break;
        }
        let raw_href = cap[1].replace("&amp;", "&");
        let title = strip_html_tags(&cap[2]);
        let snippet = snippets.get(idx).cloned().unwrap_or_default();

        if let Some(url) = unwrap_ddg_redirect(&raw_href) {
            entries.push(SerpEntry {
                url,
                title,
                snippet,
            });
        }
    }
    entries
}

fn unwrap_ddg_redirect(raw_href: &str) -> Option<Url> {
    let normalized = if raw_href.starts_with("//") {
        format!("https:{raw_href}")
    } else if raw_href.starts_with('/') {
        format!("https://duckduckgo.com{raw_href}")
    } else {
        raw_href.to_string()
    };

    let parsed = Url::parse(&normalized).ok()?;
    if parsed.host_str().unwrap_or("").contains("duckduckgo.com") {
        if let Some((_, target)) = parsed.query_pairs().find(|(k, _)| k == "uddg") {
            return Url::parse(&target).ok();
        }
    }
    if matches!(parsed.scheme(), "http" | "https") {
        Some(parsed)
    } else {
        None
    }
}

/// Meta-search client interface for dispatching orthogonal queries and retrieving facet suggestions.
pub struct MetaSearchClient {
    pub endpoint: String,
    http: Client,
}

impl MetaSearchClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(6))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            http,
        }
    }

    /// Generates the deterministic Bull / Bear / Benchmark query matrix for a node.
    pub fn build_query_matrix(
        base_query: &str,
        slot: FacetSlot,
        prompt_override: Option<&str>,
    ) -> Vec<String> {
        let base = base_query.trim();
        if let Some(ov) = prompt_override.map(str::trim).filter(|s| !s.is_empty()) {
            return vec![
                format!("{base} {ov}"),
                format!("{base} {ov} benchmark metrics"),
                format!("{base} {ov} limitations risks"),
            ];
        }

        let slot_query = match slot {
            FacetSlot::Mechanistic => format!("{base} mechanism architecture how it works"),
            FacetSlot::Quantitative => format!("{base} benchmark data metrics"),
            FacetSlot::Economic => format!("{base} unit economics cost market"),
            FacetSlot::Comparative => format!("{base} comparison alternatives tradeoffs"),
            FacetSlot::Exploratory => base.to_string(),
        };

        let mut queries = vec![
            slot_query,
            format!("{base} advantages breakthroughs performance"),
            format!("{base} limitations risks failure bottleneck criticism"),
        ];

        let mut seen = HashSet::new();
        queries.retain(|q| seen.insert(q.to_ascii_lowercase()));
        queries
    }

    /// Fetches macro search suggestions / autocomplete facets for Wave 1 seeding.
    pub async fn fetch_facets(&self, root_query: &str) -> Vec<String> {
        if !self.endpoint.is_empty() {
            if let Ok(resp) = self
                .http
                .get(format!("{}/autocompleter", self.endpoint))
                .query(&[("q", root_query)])
                .timeout(Duration::from_millis(1200))
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(arr) = json.as_array() {
                            let facets = extract_suggestion_strings(arr, root_query);
                            if !facets.is_empty() {
                                return facets;
                            }
                        }
                    }
                }
            }
        }

        if std::env::var("DDR_DISABLE_PUBLIC_SEARCH").as_deref() == Ok("1") {
            return Vec::new();
        }

        if let Ok(resp) = self
            .http
            .get("https://duckduckgo.com/ac/")
            .query(&[("q", root_query), ("type", "list")])
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = json.get(1).and_then(|v| v.as_array()) {
                        return extract_suggestion_strings(arr, root_query);
                    }
                }
            }
        }

        Vec::new()
    }

    /// Executes a targeted search query and returns top seed URLs.
    /// Order: configured SearXNG/JSON endpoint -> Brave API -> DuckDuckGo HTML -> Wikipedia API.
    pub async fn search(&self, query: &str, limit: usize) -> Vec<SerpEntry> {
        if limit == 0 || query.trim().is_empty() {
            return Vec::new();
        }

        // 1. Primary configured SearXNG / Mock SERP endpoint
        if !self.endpoint.is_empty() {
            if let Some(entries) = self.search_searxng(query, limit).await {
                if !entries.is_empty() {
                    return entries;
                }
            }
        }

        if std::env::var("DDR_DISABLE_PUBLIC_SEARCH").as_deref() == Ok("1") {
            return Vec::new();
        }

        let mut collected = Vec::new();
        let mut seen_urls = HashSet::new();

        // 2. Brave Search API (if key configured)
        if let Ok(api_key) = std::env::var("BRAVE_API_KEY") {
            if !api_key.trim().is_empty() {
                for entry in self.search_brave(query, limit, &api_key).await {
                    if seen_urls.insert(entry.url.clone()) {
                        collected.push(entry);
                    }
                }
            }
        }

        // 3. DuckDuckGo HTML (zero-config web search)
        if collected.len() < limit {
            for entry in self.search_duckduckgo(query, limit).await {
                if collected.len() >= limit {
                    break;
                }
                if seen_urls.insert(entry.url.clone()) {
                    collected.push(entry);
                }
            }
        }

        // 4. Wikipedia API fallback if still short
        if collected.len() < limit {
            for entry in self.search_wikipedia(query, limit).await {
                if collected.len() >= limit {
                    break;
                }
                if seen_urls.insert(entry.url.clone()) {
                    collected.push(entry);
                }
            }
        }

        collected
    }

    async fn search_searxng(&self, query: &str, limit: usize) -> Option<Vec<SerpEntry>> {
        let resp = self
            .http
            .get(format!("{}/search", self.endpoint))
            .query(&[("q", query), ("format", "json")])
            .timeout(Duration::from_millis(1800))
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }
        let body = resp.json::<SearxResponse>().await.ok()?;
        let entries = body
            .results
            .into_iter()
            .filter_map(|r| {
                let url = Url::parse(&r.url).ok()?;
                Some(SerpEntry {
                    url,
                    title: r.title,
                    snippet: strip_html_tags(&r.content),
                })
            })
            .take(limit)
            .collect();
        Some(entries)
    }

    async fn search_brave(&self, query: &str, limit: usize, api_key: &str) -> Vec<SerpEntry> {
        let Ok(resp) = self
            .http
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", api_key)
            .query(&[("q", query), ("count", &limit.min(20).to_string())])
            .send()
            .await
        else {
            return Vec::new();
        };

        if !resp.status().is_success() {
            return Vec::new();
        }

        let Ok(parsed) = resp.json::<BraveResponse>().await else {
            return Vec::new();
        };

        parsed
            .web
            .map(|w| w.results)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| {
                let url = Url::parse(&r.url).ok()?;
                Some(SerpEntry {
                    url,
                    title: r.title,
                    snippet: strip_html_tags(&r.description),
                })
            })
            .take(limit)
            .collect()
    }

    async fn search_duckduckgo(&self, query: &str, limit: usize) -> Vec<SerpEntry> {
        let Ok(resp) = self
            .http
            .get("https://html.duckduckgo.com/html/")
            .query(&[("q", query)])
            .send()
            .await
        else {
            debug!(%query, "DuckDuckGo search request failed");
            return Vec::new();
        };

        if !resp.status().is_success() {
            debug!(%query, status = %resp.status(), "DuckDuckGo search returned non-2xx");
            return Vec::new();
        }

        let Ok(html) = resp.text().await else {
            return Vec::new();
        };
        parse_ddg_html(&html, limit)
    }

    async fn search_wikipedia(&self, query: &str, limit: usize) -> Vec<SerpEntry> {
        let Ok(resp) = self
            .http
            .get("https://en.wikipedia.org/w/api.php")
            .header("User-Agent", "DumbDeepResearch/0.1 (research engine)")
            .query(&[
                ("action", "query"),
                ("list", "search"),
                ("srsearch", query),
                ("format", "json"),
                ("srlimit", &limit.min(10).to_string()),
            ])
            .send()
            .await
        else {
            return Vec::new();
        };

        if !resp.status().is_success() {
            return Vec::new();
        }

        let Ok(parsed) = resp.json::<WikiResponse>().await else {
            return Vec::new();
        };

        parsed
            .query
            .map(|q| q.search)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|hit| {
                let slug = hit.title.replace(' ', "_");
                let mut url = Url::parse("https://en.wikipedia.org/wiki/").ok()?;
                url.set_path(&format!("/wiki/{slug}"));
                Some(SerpEntry {
                    url,
                    title: format!("{} - Wikipedia", hit.title),
                    snippet: strip_html_tags(&hit.snippet),
                })
            })
            .take(limit)
            .collect()
    }
}

fn extract_suggestion_strings(arr: &[serde_json::Value], root_query: &str) -> Vec<String> {
    let root_lower = root_query.trim().to_ascii_lowercase();
    let mut facets = Vec::new();
    for item in arr {
        if let Some(s) = item.as_str() {
            let trimmed = s.trim();
            if !trimmed.is_empty() && trimmed.to_ascii_lowercase() != root_lower {
                facets.push(trimmed.to_string());
            }
        }
        if facets.len() >= 8 {
            break;
        }
    }
    facets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ddg_html_redirects_and_snippets() {
        let html = r#"
        <div class="result">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FSolid-state_battery&amp;rut=abc">Solid-state battery - Wikipedia</a>
          <a class="result__snippet">A solid-state battery uses solid electrodes and a <b>solid</b> electrolyte.</a>
        </div>
        <div class="result">
          <a class="result__a" href="https://energy.gov/eere/solid-state">DOE Solid-State Report</a>
          <div class="result__snippet">Cost and dendrite challenges remain barriers to scale.</div>
        </div>
        "#;

        let parsed = parse_ddg_html(html, 5);
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].url.as_str(),
            "https://en.wikipedia.org/wiki/Solid-state_battery"
        );
        assert_eq!(parsed[0].title, "Solid-state battery - Wikipedia");
        assert!(parsed[0].snippet.contains("solid electrolyte"));
        assert_eq!(parsed[1].url.as_str(), "https://energy.gov/eere/solid-state");
    }

    #[test]
    fn builds_bull_bear_benchmark_query_matrix() {
        let queries = MetaSearchClient::build_query_matrix("solid state batteries", FacetSlot::Quantitative, None);
        assert_eq!(queries.len(), 3);
        assert!(queries[0].contains("benchmark"));
        assert!(queries[1].contains("advantages"));
        assert!(queries[2].contains("limitations"));
    }
}

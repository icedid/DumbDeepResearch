use std::collections::{HashMap, HashSet};
use url::Url;

/// Enforces global crawl hygiene: canonical URL deduplication, domain ceilings, and slug blacklists.
#[derive(Default)]
pub struct FrontierGuard {
    pub visited_urls: HashSet<Url>,
    pub domain_counts: HashMap<String, usize>,
    pub max_per_domain: usize,
}

impl FrontierGuard {
    pub fn new(max_per_domain: usize) -> Self {
        Self {
            visited_urls: HashSet::new(),
            domain_counts: HashMap::new(),
            max_per_domain,
        }
    }

    /// Checks whether a candidate URL is allowed to be fetched.
    pub fn allow_url(&mut self, _url: &Url) -> bool {
        // Boilerplate stub:
        // - Reject blacklisted slugs (/tag/, /login, ?utm_*)
        // - Check visited_urls
        // - Check domain_counts < max_per_domain
        true
    }
}

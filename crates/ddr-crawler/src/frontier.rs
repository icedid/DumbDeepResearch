use std::collections::{HashMap, HashSet};
use url::Url;

const BLACKLISTED_PATH_SLUGS: &[&str] = &[
    "/tag/",
    "/tags/",
    "/category/",
    "/categories/",
    "/login",
    "/signin",
    "/sign-in",
    "/signup",
    "/sign-up",
    "/register",
    "/account",
    "/cart",
    "/checkout",
    "/wp-admin",
    "/wp-login",
    "/privacy",
    "/privacy-policy",
    "/terms",
    "/terms-of-service",
    "/cookie-policy",
    "/share",
    "/sharer",
    "/intent/tweet",
];

const BLACKLISTED_DOMAINS: &[&str] = &[
    "facebook.com",
    "twitter.com",
    "x.com",
    "instagram.com",
    "tiktok.com",
    "pinterest.com",
    "doubleclick.net",
    "googleadservices.com",
];

const TRACKING_QUERY_KEYS: &[&str] = &[
    "fbclid",
    "gclid",
    "mc_cid",
    "mc_eid",
    "ref",
    "ref_src",
    "igshid",
    "s",
    "si",
];

const MULTI_PART_SUFFIXES: &[&str] = &[
    "co.uk", "org.uk", "gov.uk", "ac.uk", "com.au", "edu.au", "gov.au", "co.nz", "govt.nz",
    "ac.nz", "co.jp", "ne.jp", "or.jp", "com.cn", "edu.cn",
];

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
            max_per_domain: max_per_domain.max(1),
        }
    }

    /// Canonicalizes a URL by enforcing HTTP(S), stripping fragments, removing tracking
    /// parameters (`utm_*`, `fbclid`, etc.), and normalizing trailing slashes.
    pub fn canonicalize_url(url: &Url) -> Option<Url> {
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let mut canonical = url.clone();
        canonical.set_fragment(None);

        if canonical.query().is_some() {
            let kept: Vec<(String, String)> = canonical
                .query_pairs()
                .filter(|(k, _)| {
                    let kl = k.to_ascii_lowercase();
                    !kl.starts_with("utm_") && !TRACKING_QUERY_KEYS.contains(&kl.as_str())
                })
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();

            if kept.is_empty() {
                canonical.set_query(None);
            } else {
                let mut pairs = canonical.query_pairs_mut();
                pairs.clear();
                for (k, v) in kept {
                    pairs.append_pair(&k, &v);
                }
            }
        }

        let trimmed_path = canonical.path().trim_end_matches('/');
        if trimmed_path.is_empty() {
            canonical.set_path("/");
        } else if trimmed_path != canonical.path() {
            let owned = trimmed_path.to_string();
            canonical.set_path(&owned);
        }

        Some(canonical)
    }

    /// Extracts the normalized root domain (eTLD+1 approximation) for domain rate-limiting.
    pub fn extract_root_domain(url: &Url) -> String {
        let host = url
            .host_str()
            .unwrap_or("")
            .trim_start_matches("www.")
            .to_ascii_lowercase();
        let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
        if parts.len() <= 2 {
            return host;
        }
        let tail_two = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
        if MULTI_PART_SUFFIXES.contains(&tail_two.as_str()) && parts.len() >= 3 {
            format!(
                "{}.{}.{}",
                parts[parts.len() - 3],
                parts[parts.len() - 2],
                parts[parts.len() - 1]
            )
        } else {
            tail_two
        }
    }

    /// Returns `true` if the URL matches noisy slug or domain blacklists.
    pub fn is_blacklisted(url: &Url) -> bool {
        let root_domain = Self::extract_root_domain(url);
        if BLACKLISTED_DOMAINS.contains(&root_domain.as_str()) {
            return true;
        }

        let path_lower = url.path().to_ascii_lowercase();
        if BLACKLISTED_PATH_SLUGS
            .iter()
            .any(|slug| path_lower.contains(slug))
        {
            return true;
        }

        false
    }

    /// Checks whether a candidate URL is allowed to be fetched, recording it if accepted.
    pub fn allow_url(&mut self, url: &Url) -> bool {
        if Self::is_blacklisted(url) {
            return false;
        }
        let Some(canonical) = Self::canonicalize_url(url) else {
            return false;
        };
        if self.visited_urls.contains(&canonical) {
            return false;
        }

        let root_domain = Self::extract_root_domain(&canonical);
        if root_domain.is_empty() {
            return false;
        }

        let current_count = self.domain_counts.get(&root_domain).copied().unwrap_or(0);
        if current_count >= self.max_per_domain {
            return false;
        }

        self.visited_urls.insert(canonical);
        *self.domain_counts.entry(root_domain).or_insert(0) += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_dedup_blacklists_and_domain_ceiling() {
        let mut guard = FrontierGuard::new(2);

        let u1 = Url::parse("https://www.example.com/research/battery?utm_source=x#top").unwrap();
        let u1_dup = Url::parse("https://www.example.com/research/battery/").unwrap();
        let u2 = Url::parse("https://blog.example.com/research/electrolyte").unwrap();
        let u3 = Url::parse("https://example.com/research/third-page").unwrap();
        let blacklisted = Url::parse("https://clean.org/tag/batteries").unwrap();
        let login = Url::parse("https://clean.org/login?next=/paper").unwrap();

        assert!(guard.allow_url(&u1));
        assert!(!guard.allow_url(&u1_dup), "canonical duplicate must be rejected");
        assert!(guard.allow_url(&u2), "second page on root domain allowed");
        assert!(!guard.allow_url(&u3), "third page on root domain exceeds ceiling 2");
        assert!(!guard.allow_url(&blacklisted), "/tag/ slug must be rejected");
        assert!(!guard.allow_url(&login), "/login slug must be rejected");
    }
}

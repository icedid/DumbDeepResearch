use lol_html::html_content::ContentType;
use lol_html::{doc_text, element, rewrite_str, text, RewriteStrSettings};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use url::Url;

const AUTHORITY_HOST_SUFFIXES: &[&str] = &[
    ".gov",
    ".edu",
    ".ac.uk",
    ".gov.uk",
    ".govt.nz",
    ".edu.au",
    "arxiv.org",
    "doi.org",
    "nature.com",
    "science.org",
    "sciencedirect.com",
    "ieee.org",
    "nih.gov",
    "sec.gov",
    "nrel.gov",
    "springer.com",
    "wiley.com",
    "ssrn.com",
];

const EVIDENCE_ANCHOR_CUES: &[&str] = &[
    "study",
    "paper",
    "filing",
    "source",
    "report",
    "benchmark",
    "audit",
    "dataset",
    "doi",
    "pdf",
    "preprint",
    "whitepaper",
    "regulation",
    "ruling",
    "patent",
    "specification",
    "citation",
    "reference",
];

fn decode_basic_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
}

fn normalize_whitespace(raw: &str) -> String {
    let decoded = decode_basic_entities(raw);
    let mut out_lines = Vec::new();
    for line in decoded.lines() {
        let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            out_lines.push(collapsed);
        }
    }
    out_lines.join("\n")
}

/// Streaming HTML sanitizer (`lol_html`) and primary citation link harvester.
pub struct DomCleaner;

impl DomCleaner {
    /// Strips boilerplate (`nav`, `footer`, `script`, `style`, `aside`, ads) and computes
    /// the immutable SHA-256 hex digest of the raw HTML snapshot.
    pub fn sanitize_html(raw_html: &str) -> (String, String) {
        let dom_sha256 = hex::encode(Sha256::digest(raw_html.as_bytes()));
        if raw_html.trim().is_empty() {
            return (String::new(), dom_sha256);
        }

        // Pass 1: Remove non-evidential DOM noise and inject structural line breaks.
        let stripped = rewrite_str(
            raw_html,
            RewriteStrSettings::new()
                .append_element_content_handler(element!(
                    "script, style, noscript, svg, iframe, nav, footer, header, aside, form, button, dialog, template, [role='navigation'], [role='banner'], [role='contentinfo'], [aria-hidden='true'], .ad, .ads, .advertisement, .sidebar, .cookie-banner, .newsletter, .comments, #comments, #nav, #footer, #header",
                    |el| {
                        el.remove();
                        Ok(())
                    }
                ))
                .append_element_content_handler(element!(
                    "p, div, li, h1, h2, h3, h4, h5, h6, tr, blockquote, article, section, br",
                    |el| {
                        el.before("\n", ContentType::Text);
                        Ok(())
                    }
                ))
                .append_element_content_handler(element!("td, th", |el| {
                    el.before(" ", ContentType::Text);
                    Ok(())
                })),
        )
        .unwrap_or_else(|_| raw_html.to_string());

        // Pass 2: Stream pure text chunks out of the cleaned HTML fragment.
        let collected = Rc::new(RefCell::new(String::new()));
        let collected_ref = Rc::clone(&collected);

        let _ = rewrite_str(
            &stripped,
            RewriteStrSettings::new().append_document_content_handler(doc_text!(|chunk| {
                collected_ref.borrow_mut().push_str(chunk.as_str());
                Ok(())
            })),
        );

        let raw_text = collected.borrow().clone();
        let clean_text = if raw_text.trim().is_empty() {
            normalize_whitespace(&stripped)
        } else {
            normalize_whitespace(&raw_text)
        };

        (clean_text, dom_sha256)
    }

    /// Extracts `<title>` (or first `<h1>`) from raw HTML.
    pub fn extract_title(raw_html: &str) -> Option<String> {
        let title_buf = Rc::new(RefCell::new(String::new()));
        let h1_buf = Rc::new(RefCell::new(String::new()));
        let t_ref = Rc::clone(&title_buf);
        let h_ref = Rc::clone(&h1_buf);

        let _ = rewrite_str(
            raw_html,
            RewriteStrSettings::new()
                .append_element_content_handler(text!("title", |chunk| {
                    t_ref.borrow_mut().push_str(chunk.as_str());
                    Ok(())
                }))
                .append_element_content_handler(text!("h1", |chunk| {
                    if h_ref.borrow().len() < 240 {
                        h_ref.borrow_mut().push_str(chunk.as_str());
                    }
                    Ok(())
                })),
        );

        let title = normalize_whitespace(&title_buf.borrow());
        if !title.is_empty() {
            return Some(title);
        }
        let h1 = normalize_whitespace(&h1_buf.borrow());
        if !h1.is_empty() {
            return Some(h1);
        }
        None
    }

    /// Extracts outbound citation links strictly from evidential containers
    /// (`<article>`, `<main>`, `<sup>`, `.references`, `section`, `p`).
    pub fn harvest_citation_links(raw_html: &str) -> Vec<Url> {
        Self::harvest_citation_links_with_base(raw_html, None)
    }

    /// Extracts outbound citation links from evidential containers, resolving relative
    /// links against `base_url` when available.
    pub fn harvest_citation_links_with_base(raw_html: &str, base_url: Option<&Url>) -> Vec<Url> {
        if raw_html.trim().is_empty() {
            return Vec::new();
        }

        // Strip nav/footer/header/aside first so footer or nav links never enter the citation set.
        let evidential_html = rewrite_str(
            raw_html,
            RewriteStrSettings::new().append_element_content_handler(element!(
                "script, style, nav, footer, header, aside, .sidebar, .comments, #nav, #footer, #header",
                |el| {
                    el.remove();
                    Ok(())
                }
            )),
        )
        .unwrap_or_else(|_| raw_html.to_string());

        let has_evidential_container = evidential_html.contains("<article")
            || evidential_html.contains("<main")
            || evidential_html.contains("<sup")
            || evidential_html.contains("references");

        let selector = if has_evidential_container {
            "article a[href], main a[href], sup a[href], .references a[href], .citations a[href], #references a[href], section a[href], p a[href]"
        } else {
            "p a[href], li a[href], a[href]"
        };

        let candidates: Rc<RefCell<Vec<(String, String, bool)>>> = Rc::new(RefCell::new(Vec::new()));
        let active_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

        let c_elem = Rc::clone(&candidates);
        let idx_elem = Rc::clone(&active_idx);
        let c_text = Rc::clone(&candidates);
        let idx_text = Rc::clone(&active_idx);

        let _ = rewrite_str(
            &evidential_html,
            RewriteStrSettings::new()
                .append_element_content_handler(element!(selector, |el| {
                    if let Some(href) = el.get_attribute("href") {
                        let class_attr = el.get_attribute("class").unwrap_or_default().to_ascii_lowercase();
                        let is_ref_class = class_attr.contains("citation") || class_attr.contains("reference");
                        let mut list = c_elem.borrow_mut();
                        let pos = list.len();
                        list.push((href, String::new(), is_ref_class));
                        *idx_elem.borrow_mut() = Some(pos);
                    }
                    Ok(())
                }))
                .append_element_content_handler(text!(selector, |chunk| {
                    let pos_opt = *idx_text.borrow();
                    if let Some(pos) = pos_opt {
                        if let Some(item) = c_text.borrow_mut().get_mut(pos) {
                            item.1.push_str(chunk.as_str());
                        }
                        if chunk.last_in_text_node() {
                            *idx_text.borrow_mut() = None;
                        }
                    }
                    Ok(())
                })),
        );

        let mut seen = HashSet::new();
        let mut results = Vec::new();

        for (href, anchor_text, is_ref_class) in candidates.borrow().iter() {
            let trimmed_href = href.trim();
            if trimmed_href.is_empty()
                || trimmed_href.starts_with('#')
                || trimmed_href.starts_with("javascript:")
                || trimmed_href.starts_with("mailto:")
            {
                continue;
            }

            let resolved = match base_url {
                Some(base) => base.join(trimmed_href).ok(),
                None => Url::parse(trimmed_href).ok(),
            };
            let Some(url) = resolved else {
                continue;
            };
            if !matches!(url.scheme(), "http" | "https") {
                continue;
            }

            if Self::is_evidential_link(&url, anchor_text, *is_ref_class) && seen.insert(url.clone())
            {
                results.push(url);
            }
        }

        results
    }

    fn is_evidential_link(url: &Url, anchor_text: &str, is_ref_class: bool) -> bool {
        let path_lower = url.path().to_ascii_lowercase();
        if path_lower.ends_with(".pdf") {
            return true;
        }

        let host_lower = url.host_str().unwrap_or("").to_ascii_lowercase();
        if AUTHORITY_HOST_SUFFIXES
            .iter()
            .any(|suffix| host_lower.ends_with(suffix))
        {
            return true;
        }

        if is_ref_class {
            return true;
        }

        let anchor_lower = anchor_text.trim().to_ascii_lowercase();
        if EVIDENCE_ANCHOR_CUES
            .iter()
            .any(|cue| anchor_lower.contains(cue))
        {
            return true;
        }

        // Bracketed inline academic citation like `[1]` or `[12]`.
        anchor_lower.starts_with('[')
            && anchor_lower.ends_with(']')
            && anchor_lower.len() <= 6
            && anchor_lower[1..anchor_lower.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_boilerplate_and_computes_deterministic_sha256() {
        let html = r#"
        <html>
          <head><title>Battery Thermal Study</title><script>console.log("ad")</script></head>
          <body>
            <nav><a href="/home">Home</a><a href="/login">Login</a></nav>
            <article>
              <h1>Solid-State Electrolyte Safety</h1>
              <p>Sulfide solid electrolytes exhibit lower flammability than liquid carbonate solvents.</p>
              <p>Benchmark tests measured 450 Wh/kg across 800 charge cycles.<a href="https://arxiv.org/pdf/2401.00001.pdf">[1]</a></p>
              <sup><a href="https://energy.gov/reports/solid-state-2025">DOE Study</a></sup>
            </article>
            <footer><a href="https://facebook.com/share">Share</a> Copyright 2026</footer>
          </body>
        </html>
        "#;

        let (clean, hash1) = DomCleaner::sanitize_html(html);
        let (_, hash2) = DomCleaner::sanitize_html(html);

        assert_eq!(hash1.len(), 64);
        assert_eq!(hash1, hash2);
        assert!(clean.contains("Sulfide solid electrolytes exhibit lower flammability"));
        assert!(!clean.contains("console.log"));
        assert!(!clean.contains("Copyright 2026"));
        assert_eq!(
            DomCleaner::extract_title(html).as_deref(),
            Some("Battery Thermal Study")
        );

        let links = DomCleaner::harvest_citation_links(html);
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|u| u.as_str().contains("arxiv.org/pdf")));
        assert!(links.iter().any(|u| u.as_str().contains("energy.gov")));
    }
}

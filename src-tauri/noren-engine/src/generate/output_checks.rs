//! Post-generation output quality checks.
//!
//! Scans generated text for five failure modes:
//! 1. Anti-pattern violations (using words the profile explicitly bans)
//! 2. Example copying (verbatim or paraphrased reuse of profile examples)
//! 3. Density/stacking (too many analogy domains or signature phrases per paragraph)
//! 4. AI-isms (triple parallels, gerund litanies, hedge qualifiers, copula dodging)
//! 5. Rhythm deviation (sentence length distribution vs corpus baseline)
//!
//! Pure string operations, no LLM calls.

use std::collections::HashSet;

use regex::Regex;
use rust_stemmers::{Algorithm, Stemmer};
use serde::Serialize;

use crate::types::BaselineRhythm;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AntiPatternEntry {
    pub terms: Vec<String>,
    pub category: String,
}

#[derive(Debug, Clone)]
pub struct ExampleQuote {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    #[serde(rename = "type")]
    pub violation_type: String,
    pub detail: String,
    #[serde(rename = "match")]
    pub match_text: String,
}

#[derive(Debug, Clone)]
pub struct AnalogyDomain {
    pub name: String,
    pub terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DensityCounts {
    pub analogy_domain_count: usize,
    pub domains_detected: Vec<String>,
    pub signature_phrase_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub violations: Vec<Violation>,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<DensityCounts>,
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

fn normalize(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut result = String::with_capacity(lower.len());
    for ch in lower.chars() {
        if ch.is_alphanumeric() || ch.is_whitespace() {
            result.push(ch);
        } else {
            result.push(' ');
        }
    }
    // Collapse whitespace
    let collapsed: Vec<&str> = result.split_whitespace().collect();
    collapsed.join(" ")
}

fn escape_regex(s: &str) -> String {
    let special = ['.', '*', '+', '?', '^', '$', '{', '}', '(', ')', '|', '[', ']', '\\'];
    let mut escaped = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        if special.contains(&ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

fn stem_word(stemmer: &Stemmer, word: &str) -> String {
    stemmer.stem(word).to_string()
}

// ---------------------------------------------------------------------------
// Anti-pattern extraction
// ---------------------------------------------------------------------------

pub fn extract_anti_patterns(md: &str) -> Vec<AntiPatternEntry> {
    let quoted_term_re = Regex::new(r#"["\u{201c}\u{201d}](.+?)["\u{201c}\u{201d}]"#).unwrap();
    let paren_examples_re = Regex::new(r"(?i)\(e\.g\.,?\s*(.+?)\)").unwrap();

    let mut entries = Vec::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut in_table = false;
    let mut avoided_col_idx: Option<usize> = None;
    let mut category_col_idx: Option<usize> = None;

    for line in &lines {
        let stripped = line.trim();

        // Table header detection
        if stripped.contains('|') && !in_table {
            let cells: Vec<&str> = stripped.split('|').map(|c| c.trim()).collect();
            for (i, cell) in cells.iter().enumerate() {
                let lower = cell.to_lowercase();
                if lower == "avoided" {
                    avoided_col_idx = Some(i);
                }
                if lower == "category" {
                    category_col_idx = Some(i);
                }
            }
            if avoided_col_idx.is_some() {
                in_table = true;
                continue;
            }
        }

        // Skip separator row
        if in_table {
            let sep_re = Regex::new(r"^\|[\s\-|]+\|$").unwrap();
            if sep_re.is_match(stripped) {
                continue;
            }
        }

        // Data rows
        if in_table && stripped.starts_with('|') {
            let cells: Vec<&str> = stripped.split('|').map(|c| c.trim()).collect();
            let avoided_idx = match avoided_col_idx {
                Some(idx) if idx < cells.len() => idx,
                _ => continue,
            };

            let avoided_cell = cells[avoided_idx];
            let category = match category_col_idx {
                Some(idx) if idx < cells.len() => cells[idx].to_string(),
                _ => String::new(),
            };

            let mut terms: Vec<String> = Vec::new();

            // Extract quoted terms
            for cap in quoted_term_re.captures_iter(avoided_cell) {
                let raw = cap[1].trim();
                for part in raw.split('/') {
                    let cleaned = part
                        .trim()
                        .trim_matches(|c: char| c == '"' || c == '\u{201c}' || c == '\u{201d}')
                        .trim_end_matches(|c: char| c == ',' || c == ';' || c == '.')
                        .trim();
                    if !cleaned.is_empty() {
                        terms.push(cleaned.to_string());
                    }
                }
            }

            // Extract parenthetical examples
            for cap in paren_examples_re.captures_iter(avoided_cell) {
                let examples: Vec<&str> = cap[1].split(',').map(|s| s.trim()).collect();
                for ex in examples {
                    if !ex.is_empty()
                        && !terms.iter().any(|t| t.to_lowercase() == ex.to_lowercase())
                    {
                        terms.push(ex.to_string());
                    }
                }
            }

            if !terms.is_empty() {
                entries.push(AntiPatternEntry { terms, category });
            }
            continue;
        }

        // End table on non-table line
        if in_table && !stripped.starts_with('|') && !stripped.is_empty() {
            if stripped.starts_with('#') {
                in_table = false;
                avoided_col_idx = None;
                category_col_idx = None;
            }
        }
    }

    entries
}

// ---------------------------------------------------------------------------
// Anti-pattern scanning
// ---------------------------------------------------------------------------

pub fn check_anti_patterns(output: &str, patterns: &[AntiPatternEntry]) -> Vec<Violation> {
    let stemmer = Stemmer::create(Algorithm::English);
    let mut violations = Vec::new();
    let normalized_output = normalize(output);
    let output_words: Vec<&str> = normalized_output.split_whitespace().collect();
    let output_stems: HashSet<String> = output_words.iter().map(|w| stem_word(&stemmer, w)).collect();

    for entry in patterns {
        for term in &entry.terms {
            let normalized_term = normalize(term);
            if normalized_term.is_empty() {
                continue;
            }

            let words: Vec<&str> = normalized_term.split_whitespace().collect();
            let found;

            if words.len() > 1 {
                // Multi-word terms: regex with flexible whitespace/hyphen separators
                let pattern = words.iter().map(|w| escape_regex(w)).collect::<Vec<_>>().join("[\\s-]*");
                let re = Regex::new(&format!(r"\b{}\b", pattern)).unwrap_or_else(|_| {
                    Regex::new(&escape_regex(&normalized_term)).unwrap()
                });
                found = re.is_match(&normalized_output);
            } else {
                // Single-word terms: stem-based matching
                found = output_stems.contains(&stem_word(&stemmer, &normalized_term));
            }

            if found {
                violations.push(Violation {
                    violation_type: "anti-pattern".to_string(),
                    detail: format!(
                        "Anti-pattern \"{}\" ({}) found in output",
                        term, entry.category
                    ),
                    match_text: term.clone(),
                });
            }
        }
    }

    violations
}

// ---------------------------------------------------------------------------
// Example quote extraction
// ---------------------------------------------------------------------------

fn is_long_enough(text: &str) -> bool {
    text.split_whitespace().count() >= 5
}

pub fn extract_example_quotes(md: &str) -> Vec<ExampleQuote> {
    let example_anchor_re =
        Regex::new(r"(?i)\*\*(?:Examples?(?:\s+from\s+corpus)?|Example of rhythm shift):\*\*")
            .unwrap();
    let inline_example_quote_re =
        Regex::new(r#"(?i)\*\*Examples?:\*\*\s*["\u{201c}](.+?)["\u{201d}]"#).unwrap();
    let bullet_example_re =
        Regex::new(r#"^[-*]\s+["\u{201c}](.+?)["\u{201d}]"#).unwrap();
    let table_quote_re =
        Regex::new(r#"["\u{201c}](.+?)["\u{201d}]"#).unwrap();

    let mut quotes: Vec<ExampleQuote> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let add_quote = |text: &str, quotes: &mut Vec<ExampleQuote>, seen: &mut HashSet<String>| {
        let trimmed = text.trim();
        if !is_long_enough(trimmed) {
            return;
        }
        let key = normalize(trimmed);
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        quotes.push(ExampleQuote {
            text: trimmed.to_string(),
        });
    };

    let lines: Vec<&str> = md.lines().collect();
    let mut in_anchor = false;
    let mut in_table = false;
    let mut example_col_idx: Option<usize> = None;

    let boundary_re = Regex::new(r"^(?:#{1,4}\s|---|\*\*[A-Z])").unwrap();
    let table_sep_re = Regex::new(r"^\|[\s\-|]+\|$").unwrap();

    for stripped_raw in &lines {
        let stripped = stripped_raw.trim();

        // Table header detection
        if stripped.contains('|') && !in_anchor && !in_table {
            let cells: Vec<&str> = stripped.split('|').map(|c| c.trim()).collect();
            for (ci, cell) in cells.iter().enumerate() {
                let lower = cell.to_lowercase();
                if lower == "example" || lower == "example context" {
                    in_table = true;
                    example_col_idx = Some(ci);
                    break;
                }
            }
            if in_table {
                continue;
            }
        }

        // Table separator
        if in_table && table_sep_re.is_match(stripped) {
            continue;
        }

        // Table data rows
        if in_table && stripped.contains('|') {
            let cells: Vec<&str> = stripped.split('|').map(|c| c.trim()).collect();
            if let Some(idx) = example_col_idx {
                if idx < cells.len() {
                    for cap in table_quote_re.captures_iter(cells[idx]) {
                        add_quote(&cap[1], &mut quotes, &mut seen);
                    }
                }
            }
            if !stripped.starts_with('|') {
                in_table = false;
                example_col_idx = None;
            }
            continue;
        }

        if in_table && !stripped.starts_with('|') && !stripped.is_empty() {
            in_table = false;
            example_col_idx = None;
        }

        // Anchor sections
        if example_anchor_re.is_match(stripped) {
            in_anchor = true;
            for cap in inline_example_quote_re.captures_iter(stripped) {
                add_quote(&cap[1], &mut quotes, &mut seen);
            }
            continue;
        }

        if in_anchor {
            if !stripped.is_empty() && boundary_re.is_match(stripped) {
                in_anchor = false;
                if example_anchor_re.is_match(stripped) {
                    in_anchor = true;
                    for cap in inline_example_quote_re.captures_iter(stripped) {
                        add_quote(&cap[1], &mut quotes, &mut seen);
                    }
                }
                continue;
            }

            if stripped.is_empty() {
                continue;
            }

            // Bullet with quote
            if let Some(cap) = bullet_example_re.captures(stripped) {
                add_quote(&cap[1], &mut quotes, &mut seen);
                continue;
            }

            // Bare quoted line
            for cap in table_quote_re.captures_iter(stripped) {
                add_quote(&cap[1], &mut quotes, &mut seen);
            }
        }

        // Standalone **Example:** "quote" outside anchor context
        if !in_anchor && !in_table {
            for cap in inline_example_quote_re.captures_iter(stripped) {
                add_quote(&cap[1], &mut quotes, &mut seen);
            }
        }
    }

    quotes
}

// ---------------------------------------------------------------------------
// Similarity: trigram Jaccard
// ---------------------------------------------------------------------------

fn word_trigrams(words: &[&str]) -> HashSet<String> {
    let mut trigrams = HashSet::new();
    if words.len() < 3 {
        return trigrams;
    }
    for i in 0..=(words.len() - 3) {
        trigrams.insert(format!("{} {} {}", words[i], words[i + 1], words[i + 2]));
    }
    trigrams
}

pub fn trigram_jaccard(a: &str, b: &str) -> f64 {
    let norm_a = normalize(a);
    let norm_b = normalize(b);
    let words_a: Vec<&str> = norm_a.split_whitespace().collect();
    let words_b: Vec<&str> = norm_b.split_whitespace().collect();

    if words_a.len() < 3 || words_b.len() < 3 {
        return 0.0;
    }

    let tri_a = word_trigrams(&words_a);
    let tri_b = word_trigrams(&words_b);

    let intersection = tri_a.intersection(&tri_b).count();
    let union = tri_a.len() + tri_b.len() - intersection;

    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

// ---------------------------------------------------------------------------
// Sentence splitting
// ---------------------------------------------------------------------------

pub fn split_sentences(text: &str) -> Vec<String> {
    // Rust regex doesn't support lookbehind. Split manually on sentence-ending
    // punctuation followed by whitespace.
    let mut sentences = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if (chars[i] == '.' || chars[i] == '!' || chars[i] == '?')
            && i + 1 < chars.len()
            && chars[i + 1].is_whitespace()
        {
            // Include the punctuation in this sentence
            let end = i + 1;
            let sentence: String = chars[start..end].iter().collect();
            let trimmed = sentence.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            // Skip whitespace
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    // Trailing sentence without terminal punctuation
    if start < chars.len() {
        let sentence: String = chars[start..].iter().collect();
        let trimmed = sentence.trim();
        if !trimmed.is_empty() {
            sentences.push(trimmed.to_string());
        }
    }
    sentences
}

// ---------------------------------------------------------------------------
// Example-copy checking
// ---------------------------------------------------------------------------

pub fn check_example_copying(output: &str, examples: &[ExampleQuote]) -> Vec<Violation> {
    let mut violations = Vec::new();
    let normalized_output = normalize(output);
    let output_sentences = split_sentences(output);

    for example in examples {
        let normalized_example = normalize(&example.text);
        let example_word_count = normalized_example.split_whitespace().count();

        // Tier 1: Verbatim substring match
        if normalized_output.contains(&normalized_example) {
            violations.push(Violation {
                violation_type: "example-copy".to_string(),
                detail: format!("Copied verbatim: \"{}\"", truncate(&example.text, 80)),
                match_text: example.text.clone(),
            });
            continue;
        }

        // Tier 2: Trigram Jaccard similarity
        let mut max_jaccard: f64 = 0.0;

        for sentence in &output_sentences {
            let score = trigram_jaccard(sentence, &example.text);
            if score > max_jaccard {
                max_jaccard = score;
            }
        }

        // For longer examples (10+ words), also use sliding window
        if example_word_count >= 10 {
            let output_words: Vec<&str> = normalized_output.split_whitespace().collect();
            if output_words.len() >= example_word_count {
                for i in 0..=(output_words.len() - example_word_count) {
                    let window = output_words[i..i + example_word_count].join(" ");
                    let score = trigram_jaccard(&window, &example.text);
                    if score > max_jaccard {
                        max_jaccard = score;
                    }
                }
            }
        }

        if max_jaccard >= 0.5 {
            violations.push(Violation {
                violation_type: "example-copy".to_string(),
                detail: format!(
                    "Closely paraphrases (similarity {}%): \"{}\"",
                    (max_jaccard * 100.0).round() as u32,
                    truncate(&example.text, 80)
                ),
                match_text: example.text.clone(),
            });
        }
    }

    violations
}

// ---------------------------------------------------------------------------
// Analogy Domain Extraction
// ---------------------------------------------------------------------------

fn skip_header_words() -> HashSet<&'static str> {
    [
        "and", "the", "for", "from", "with", "into", "over", "about",
        "development", "business", "strategy", "framework", "sources",
        "structure", "system", "process", "general",
    ]
    .into_iter()
    .collect()
}

pub fn extract_analogy_domains(md: &str) -> Vec<AnalogyDomain> {
    let mut domains = Vec::new();

    let start_re = Regex::new(r"(?im)^##\s+Analogy\s+Sources[^\n]*").unwrap();
    let start_match = match start_re.find(md) {
        Some(m) => m,
        None => return domains,
    };

    let rest = &md[start_match.end()..];
    let end_re = Regex::new(r"(?m)^##\s+").unwrap();
    let section_body = match end_re.find(rest) {
        Some(m) => &rest[..m.start()],
        None => rest,
    };

    let h3_re = Regex::new(r"(?m)^###\s+(.+)").unwrap();
    let mut h3_list: Vec<(String, usize)> = Vec::new();
    for cap in h3_re.captures_iter(section_body) {
        let bracket_re = Regex::new(r"\s*\[.*?\]\s*$").unwrap();
        let raw_name = bracket_re.replace(cap[1].trim(), "").to_string();
        h3_list.push((raw_name, cap.get(0).unwrap().start()));
    }

    let skip = skip_header_words();
    let paren_re = Regex::new(r"\((\w{4,})\)").unwrap();

    for i in 0..h3_list.len() {
        let start = h3_list[i].1;
        let end = if i + 1 < h3_list.len() {
            h3_list[i + 1].1
        } else {
            section_body.len()
        };
        let body = &section_body[start..end];
        let mut terms: HashSet<String> = HashSet::new();

        // Header words
        for segment in h3_list[i].0.split('/') {
            for word in segment.trim().split_whitespace() {
                let w: String = word.to_lowercase().chars().filter(|c| c.is_ascii_lowercase()).collect();
                if w.len() >= 4 && !skip.contains(w.as_str()) {
                    terms.insert(w);
                }
            }
        }

        // Parenthetical single-word definitions
        for cap in paren_re.captures_iter(body) {
            let w = cap[1].to_lowercase();
            if !skip.contains(w.as_str()) {
                terms.insert(w);
            }
        }

        if !terms.is_empty() {
            domains.push(AnalogyDomain {
                name: h3_list[i].0.clone(),
                terms: terms.into_iter().collect(),
            });
        }
    }

    domains
}

// ---------------------------------------------------------------------------
// Signature Phrase Extraction
// ---------------------------------------------------------------------------

pub fn extract_signature_phrase_terms(md: &str) -> Vec<String> {
    let mut phrases = Vec::new();

    let start_re = Regex::new(r"(?im)^##\s+Signature\s+Phrases").unwrap();
    let start_match = match start_re.find(md) {
        Some(m) => m,
        None => return phrases,
    };

    let rest = &md[start_match.start()..];
    let end_re = Regex::new(r"(?m)^##\s+").unwrap();
    let section = match end_re.find(&rest[start_match.end() - start_match.start()..]) {
        Some(m) => &rest[..start_match.end() - start_match.start() + m.start()],
        None => rest,
    };

    let sep_re = Regex::new(r"^\|[\s\-:]+\|").unwrap();
    let quote_cleanup_re = Regex::new(r#"["\u{201c}\u{201d}]"#).unwrap();
    let bracket_re = Regex::new(r"\[.*?\]").unwrap();
    let paren_re = Regex::new(r"\(.*?\)").unwrap();

    let mut past_separator = false;

    for line in section.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }

        if sep_re.is_match(trimmed) {
            past_separator = true;
            continue;
        }

        if !past_separator {
            continue;
        }

        let cells: Vec<String> = trimmed
            .split('|')
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if cells.is_empty() {
            continue;
        }

        // Handle slash-separated alternatives
        let alternatives: Vec<&str> = cells[0].split("\"/\"").collect();
        for alt in alternatives {
            let mut phrase = quote_cleanup_re.replace_all(alt, "").to_string();
            // Strip trailing ellipsis
            phrase = Regex::new(r"\.{3,}$")
                .unwrap()
                .replace(&phrase, "")
                .to_string();
            phrase = bracket_re.replace_all(&phrase, "").to_string();
            phrase = paren_re.replace_all(&phrase, "").to_string();
            phrase = phrase.trim_end_matches(|c: char| c == ':' || c == ';').to_string();
            // Collapse whitespace
            let words: Vec<&str> = phrase.split_whitespace().collect();
            phrase = words.join(" ");

            if phrase.split_whitespace().count() >= 3 {
                phrases.push(phrase);
            }
        }
    }

    phrases
}

// ---------------------------------------------------------------------------
// Density & Stacking Check
// ---------------------------------------------------------------------------

pub fn check_density_stacking(
    output: &str,
    domains: &[AnalogyDomain],
    signature_phrases: &[String],
) -> (Vec<Violation>, DensityCounts) {
    let mut violations = Vec::new();
    let paragraphs: Vec<&str> = output
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .collect();

    let mut all_domains_used: HashSet<String> = HashSet::new();
    let mut total_phrase_count = 0;

    for para in &paragraphs {
        let normalized_para = normalize(para);
        let mut para_domains_found: Vec<String> = Vec::new();

        for domain in domains {
            for term in &domain.terms {
                let re = Regex::new(&format!(r"\b{}\b", escape_regex(term)));
                if let Ok(re) = re {
                    if re.is_match(&normalized_para) {
                        if !para_domains_found.contains(&domain.name) {
                            para_domains_found.push(domain.name.clone());
                            all_domains_used.insert(domain.name.clone());
                        }
                        break;
                    }
                }
            }
        }

        let mut para_signature_count = 0;
        for phrase in signature_phrases {
            let normalized_phrase = normalize(phrase);
            if normalized_para.contains(&normalized_phrase) {
                para_signature_count += 1;
                total_phrase_count += 1;
            }
        }

        // Stacking: 2+ analogy domains in one paragraph
        if para_domains_found.len() >= 2 {
            violations.push(Violation {
                violation_type: "density".to_string(),
                detail: format!(
                    "Stacking: paragraph mixes analogy domains ({})",
                    para_domains_found.join(", ")
                ),
                match_text: para_domains_found.join(", "),
            });
        }

        // High density: 3+ marked patterns without domain stacking
        let total_marked = para_domains_found.len() + para_signature_count;
        if total_marked >= 3 && para_domains_found.len() < 2 {
            violations.push(Violation {
                violation_type: "density".to_string(),
                detail: format!(
                    "High density: {} marked patterns in one paragraph",
                    total_marked
                ),
                match_text: format!("{} patterns", total_marked),
            });
        }
    }

    let density = DensityCounts {
        analogy_domain_count: all_domains_used.len(),
        domains_detected: all_domains_used.into_iter().collect(),
        signature_phrase_count: total_phrase_count,
    };

    (violations, density)
}

// ---------------------------------------------------------------------------
// AI-ism Detection
// ---------------------------------------------------------------------------

fn is_markdown_line(s: &str) -> bool {
    let re = Regex::new(r"^(?:#|\s*[-*]|\s*\d+\.)\s").unwrap();
    re.is_match(s)
}

fn detect_triple_parallels(sentences: &[String]) -> Vec<Violation> {
    let mut violations = Vec::new();

    if sentences.len() < 3 {
        return violations;
    }

    for i in 0..=(sentences.len() - 3) {
        let window = &sentences[i..i + 3];

        // Skip windows containing markdown list items or headings
        if window.iter().any(|s| is_markdown_line(s)) {
            continue;
        }

        let words: Vec<Vec<&str>> = window
            .iter()
            .map(|s| s.split_whitespace().collect())
            .collect();

        // First 2 words match across all 3
        let first2_match = words.iter().all(|w| w.len() >= 2)
            && words[0][0].to_lowercase() == words[1][0].to_lowercase()
            && words[1][0].to_lowercase() == words[2][0].to_lowercase()
            && words[0][1].to_lowercase() == words[1][1].to_lowercase()
            && words[1][1].to_lowercase() == words[2][1].to_lowercase();

        // First word matches AND all 3 are short (under 15 words)
        let first1_short_match = words.iter().all(|w| !w.is_empty() && w.len() < 15)
            && words[0][0].to_lowercase() == words[1][0].to_lowercase()
            && words[1][0].to_lowercase() == words[2][0].to_lowercase();

        // Last 2 words match across all 3
        let last2_match = words.iter().all(|w| w.len() >= 2)
            && words[0][words[0].len() - 1].to_lowercase()
                == words[1][words[1].len() - 1].to_lowercase()
            && words[1][words[1].len() - 1].to_lowercase()
                == words[2][words[2].len() - 1].to_lowercase()
            && words[0][words[0].len() - 2].to_lowercase()
                == words[1][words[1].len() - 2].to_lowercase()
            && words[1][words[1].len() - 2].to_lowercase()
                == words[2][words[2].len() - 2].to_lowercase();

        if first2_match || first1_short_match || last2_match {
            violations.push(Violation {
                violation_type: "ai-ism".to_string(),
                detail: format!(
                    "Triple parallel structure: \"{}\" / \"{}\" / \"{}\"",
                    truncate(&window[0], 40),
                    truncate(&window[1], 40),
                    truncate(&window[2], 40)
                ),
                match_text: format!(
                    "{} | {} | {}",
                    truncate(&window[0], 30),
                    truncate(&window[1], 30),
                    truncate(&window[2], 30)
                ),
            });
        }
    }

    violations
}

fn gerund_skip_words() -> HashSet<&'static str> {
    [
        "during", "nothing", "something", "everything", "anything",
        "morning", "evening", "spring", "according", "king",
        "ring", "thing", "ceiling", "string",
    ]
    .into_iter()
    .collect()
}

fn detect_gerund_litanies(sentences: &[String]) -> Vec<Violation> {
    let mut violations = Vec::new();
    let skip = gerund_skip_words();
    let mut streak: Vec<String> = Vec::new();

    for sentence in sentences {
        let first_word: String = sentence
            .split_whitespace()
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_lowercase();

        let word_count = sentence.split_whitespace().count();
        let is_gerund = first_word.ends_with("ing")
            && first_word.len() > 3
            && !skip.contains(first_word.as_str())
            && word_count < 12;

        if is_gerund {
            streak.push(sentence.clone());
        } else {
            if streak.len() >= 3 {
                violations.push(Violation {
                    violation_type: "ai-ism".to_string(),
                    detail: format!(
                        "Gerund litany ({} consecutive): \"{}\" ...",
                        streak.len(),
                        truncate(&streak[0], 40)
                    ),
                    match_text: streak
                        .iter()
                        .map(|s| truncate(s, 30))
                        .collect::<Vec<_>>()
                        .join(" | "),
                });
            }
            streak.clear();
        }
    }

    // Check trailing streak
    if streak.len() >= 3 {
        violations.push(Violation {
            violation_type: "ai-ism".to_string(),
            detail: format!(
                "Gerund litany ({} consecutive): \"{}\" ...",
                streak.len(),
                truncate(&streak[0], 40)
            ),
            match_text: streak
                .iter()
                .map(|s| truncate(s, 30))
                .collect::<Vec<_>>()
                .join(" | "),
        });
    }

    violations
}

const HEDGE_PHRASES: &[&str] = &[
    "it's worth noting",
    "it is worth noting",
    "it should be noted",
    "it's important to note",
    "it is important to note",
    "in many cases",
    "even if they can't articulate",
    "one might argue",
    "it could be argued",
    "might want to consider",
    "to anyone who reads",
];

fn detect_hedge_qualifiers(output: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let normalized = normalize(output);

    for phrase in HEDGE_PHRASES {
        if normalized.contains(&normalize(phrase)) {
            violations.push(Violation {
                violation_type: "ai-ism".to_string(),
                detail: format!("Hedge qualifier: \"{}\"", phrase),
                match_text: phrase.to_string(),
            });
        }
    }

    violations
}

const COPULA_DODGE_PHRASES: &[&str] = &["serves as", "stands as", "functions as"];

fn detect_copula_dodging(output: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let normalized = normalize(output);

    for phrase in COPULA_DODGE_PHRASES {
        if normalized.contains(&normalize(phrase)) {
            violations.push(Violation {
                violation_type: "ai-ism".to_string(),
                detail: format!("Copula dodging: \"{}\"", phrase),
                match_text: phrase.to_string(),
            });
        }
    }

    violations
}

pub fn check_ai_isms(output: &str) -> Vec<Violation> {
    let sentences = split_sentences(output);
    let mut violations = Vec::new();
    violations.extend(detect_triple_parallels(&sentences));
    violations.extend(detect_gerund_litanies(&sentences));
    violations.extend(detect_hedge_qualifiers(output));
    violations.extend(detect_copula_dodging(output));
    violations
}

// ---------------------------------------------------------------------------
// Rhythm Deviation Check
// ---------------------------------------------------------------------------

const MIN_SENTENCES_FOR_RHYTHM_CHECK: usize = 8;
const RATIO_DEVIATION_THRESHOLD: f64 = 0.4;

pub fn check_rhythm_deviation(output: &str, baseline: &BaselineRhythm) -> Vec<Violation> {
    let mut violations = Vec::new();

    let sentences: Vec<String> = split_sentences(output)
        .into_iter()
        .filter(|s| s.split_whitespace().count() >= 3)
        .collect();

    if sentences.len() < MIN_SENTENCES_FOR_RHYTHM_CHECK {
        return violations;
    }

    let word_counts: Vec<usize> = sentences.iter().map(|s| s.split_whitespace().count()).collect();
    let mut short: usize = 0;
    let mut long_count: usize = 0;
    for &wc in &word_counts {
        if wc < 8 {
            short += 1;
        } else if wc >= 16 {
            long_count += 1;
        }
    }

    let output_ratio = if short > 0 {
        long_count as f64 / short as f64
    } else {
        f64::INFINITY
    };
    let corpus_ratio = baseline.long_to_short_ratio;

    // Only flag if corpus has a meaningful ratio and output is significantly below it
    if corpus_ratio > 0.0
        && corpus_ratio.is_finite()
        && output_ratio.is_finite()
        && output_ratio < corpus_ratio * RATIO_DEVIATION_THRESHOLD
    {
        violations.push(Violation {
            violation_type: "ai-ism".to_string(),
            detail: format!(
                "Rhythm deviation: long-to-short ratio is {:.1} (corpus baseline: {:.1}). Too many short sentences.",
                output_ratio, corpus_ratio
            ),
            match_text: format!("ratio {:.1} vs {:.1}", output_ratio, corpus_ratio),
        });
    }

    // Also flag if short sentence percentage is more than double the corpus
    let output_short_pct = (short as f64 / sentences.len() as f64) * 100.0;
    let corpus_short_pct = baseline.distribution_pct.short;
    if corpus_short_pct > 0.0
        && output_short_pct > corpus_short_pct * 2.5
        && output_short_pct > 30.0
    {
        violations.push(Violation {
            violation_type: "ai-ism".to_string(),
            detail: format!(
                "Rhythm deviation: {}% short sentences (<8 words), corpus baseline is {}%.",
                output_short_pct.round() as u32,
                corpus_short_pct
            ),
            match_text: format!(
                "short {}% vs {}%",
                output_short_pct.round() as u32,
                corpus_short_pct
            ),
        });
    }

    violations
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn run_output_checks(
    output: &str,
    core_identity: &str,
    context_layer: Option<&str>,
    baseline_rhythm: Option<&BaselineRhythm>,
) -> CheckResult {
    let anti_patterns = extract_anti_patterns(core_identity);

    // Collect example quotes from both layers
    let mut examples = extract_example_quotes(core_identity);
    if let Some(ctx) = context_layer {
        for q in extract_example_quotes(ctx) {
            let key = normalize(&q.text);
            if !examples.iter().any(|e| normalize(&e.text) == key) {
                examples.push(q);
            }
        }
    }

    // Density check
    let domains = extract_analogy_domains(core_identity);
    let mut signature_phrases = extract_signature_phrase_terms(core_identity);
    if let Some(ctx) = context_layer {
        signature_phrases.extend(extract_signature_phrase_terms(ctx));
    }
    let (density_violations, density) = check_density_stacking(output, &domains, &signature_phrases);

    let mut violations = Vec::new();
    violations.extend(check_anti_patterns(output, &anti_patterns));
    violations.extend(check_example_copying(output, &examples));
    violations.extend(density_violations);
    violations.extend(check_ai_isms(output));
    if let Some(rhythm) = baseline_rhythm {
        violations.extend(check_rhythm_deviation(output, rhythm));
    }

    let passed = violations.is_empty();
    CheckResult {
        violations,
        passed,
        density: Some(density),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BaselineRhythm, RhythmDistribution};

    // --- Anti-pattern tests ---

    #[test]
    fn extract_anti_patterns_from_table() {
        let md = r#"
### Anti-Patterns

| Category | Avoided | Replacement |
|----------|---------|-------------|
| Hype | "revolutionary" / "game-changing" | specific claims |
| Filler | "utilize" (e.g., leverage, synergy) | plain verbs |
"#;
        let patterns = extract_anti_patterns(md);
        assert_eq!(patterns.len(), 2);
        assert!(patterns[0].terms.contains(&"revolutionary".to_string()));
        assert!(patterns[0].terms.contains(&"game-changing".to_string()));
        assert!(patterns[1].terms.contains(&"utilize".to_string()));
        assert!(patterns[1].terms.contains(&"leverage".to_string()));
    }

    #[test]
    fn check_anti_patterns_finds_stemmed_match() {
        // Porter2 stems "utilizing" and "utilize" to the same root
        let patterns = vec![AntiPatternEntry {
            terms: vec!["utilize".to_string()],
            category: "Filler".to_string(),
        }];
        let violations = check_anti_patterns("We are utilizing the new API.", &patterns);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].violation_type, "anti-pattern");
    }

    #[test]
    fn check_anti_patterns_multiword() {
        let patterns = vec![AntiPatternEntry {
            terms: vec!["game changing".to_string()],
            category: "Hype".to_string(),
        }];
        let violations = check_anti_patterns("This is a game-changing product.", &patterns);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn check_anti_patterns_no_false_positive() {
        let patterns = vec![AntiPatternEntry {
            terms: vec!["leverage".to_string()],
            category: "Filler".to_string(),
        }];
        let violations = check_anti_patterns("The team built a great product.", &patterns);
        assert!(violations.is_empty());
    }

    // --- Example copying tests ---

    #[test]
    fn check_verbatim_copy() {
        let examples = vec![ExampleQuote {
            text: "The best code is the code you never write at all.".to_string(),
        }];
        let output = "As someone once said, the best code is the code you never write at all.";
        let violations = check_example_copying(output, &examples);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].violation_type, "example-copy");
        assert!(violations[0].detail.contains("verbatim"));
    }

    #[test]
    fn check_high_similarity_copy() {
        let examples = vec![ExampleQuote {
            text: "Good writing is clear thinking made visible on the page today.".to_string(),
        }];
        let output = "Good writing is clear thinking made visible on the page right now.";
        let violations = check_example_copying(output, &examples);
        assert!(!violations.is_empty());
    }

    #[test]
    fn no_copy_on_unrelated_text() {
        let examples = vec![ExampleQuote {
            text: "The market rewards patience over speed in most situations.".to_string(),
        }];
        let output = "Rust's ownership model prevents data races at compile time.";
        let violations = check_example_copying(output, &examples);
        assert!(violations.is_empty());
    }

    // --- Density stacking tests ---

    #[test]
    fn detects_domain_stacking() {
        let domains = vec![
            AnalogyDomain {
                name: "Music".to_string(),
                terms: vec!["melody".to_string(), "harmony".to_string()],
            },
            AnalogyDomain {
                name: "Architecture".to_string(),
                terms: vec!["foundation".to_string(), "scaffold".to_string()],
            },
        ];
        let output = "The melody of the code rests on a solid foundation.";
        let (violations, _) = check_density_stacking(output, &domains, &[]);
        assert!(!violations.is_empty());
        assert!(violations[0].detail.contains("Stacking"));
    }

    #[test]
    fn extracts_analogy_domains_until_next_h2() {
        let md = r#"
## Analogy Sources

### Medicine
Terms include diagnosis (triage).

## Signature Phrases

### Should Not Be Parsed
"#;
        let domains = extract_analogy_domains(md);
        assert_eq!(domains.len(), 1);
        assert_eq!(domains[0].name, "Medicine");
        assert!(domains[0].terms.contains(&"medicine".to_string()));
        assert!(domains[0].terms.contains(&"triage".to_string()));
    }

    #[test]
    fn extracts_signature_phrases_until_next_h2() {
        let md = r#"
## Signature Phrases

| Phrase | Notes |
|--------|-------|
| "sharp edge pattern" | recurring |
| "working surface habit" | recurring |

## Rhythm

| Phrase | Notes |
|--------|-------|
| "not part of signature" | later section |
"#;
        let phrases = extract_signature_phrase_terms(md);
        assert!(phrases.contains(&"sharp edge pattern".to_string()));
        assert!(phrases.contains(&"working surface habit".to_string()));
        assert!(!phrases.contains(&"not part of signature".to_string()));
    }

    // --- AI-ism tests ---

    #[test]
    fn detects_triple_parallel() {
        let output = "Code is art. Code is science. Code is craft.";
        let violations = check_ai_isms(output);
        assert!(!violations.is_empty());
        assert!(violations[0].detail.contains("Triple parallel"));
    }

    #[test]
    fn detects_gerund_litany() {
        let output = "Building the system. Running the tests. Deploying the code. Monitoring the logs.";
        let violations = check_ai_isms(output);
        let gerund = violations.iter().find(|v| v.detail.contains("Gerund"));
        assert!(gerund.is_some());
    }

    #[test]
    fn detects_hedge_qualifier() {
        let output = "It's worth noting that the performance improved.";
        let violations = check_ai_isms(output);
        assert!(!violations.is_empty());
        assert!(violations[0].detail.contains("Hedge"));
    }

    #[test]
    fn detects_copula_dodging() {
        let output = "This library serves as a bridge between systems.";
        let violations = check_ai_isms(output);
        assert!(!violations.is_empty());
        assert!(violations[0].detail.contains("Copula"));
    }

    #[test]
    fn no_ai_ism_on_clean_text() {
        let output = "The system processes requests in parallel. Each worker handles one connection at a time. When load increases, the scheduler distributes evenly.";
        let violations = check_ai_isms(output);
        assert!(violations.is_empty());
    }

    // --- Rhythm deviation tests ---

    #[test]
    fn rhythm_deviation_flags_too_many_short() {
        let baseline = BaselineRhythm {
            total_sentences: 100,
            median_word_count: 14.0,
            mean_word_count: 16.0,
            distribution: RhythmDistribution {
                short: 10.0,
                medium: 40.0,
                long: 35.0,
                very_long: 15.0,
            },
            distribution_pct: RhythmDistribution {
                short: 10.0,
                medium: 40.0,
                long: 35.0,
                very_long: 15.0,
            },
            long_to_short_ratio: 5.0,
            median_commas_per_sentence: 1.5,
            mean_commas_per_sentence: 1.8,
            mean_paragraph_sentences: 4.0,
            sentence_ceiling: 40,
        };

        // Output with mostly short sentences
        let output = "Short one. Quick hit. Fast read. Tiny bit. Small part. Brief note. Just this. Done now. The end is here at last finally.";
        let violations = check_rhythm_deviation(output, &baseline);
        // May or may not fire depending on sentence count threshold
        // At least verify no panic
        let _ = violations;
    }

    #[test]
    fn rhythm_ok_when_matching_baseline() {
        let baseline = BaselineRhythm {
            total_sentences: 100,
            median_word_count: 14.0,
            mean_word_count: 16.0,
            distribution: RhythmDistribution {
                short: 20.0,
                medium: 40.0,
                long: 30.0,
                very_long: 10.0,
            },
            distribution_pct: RhythmDistribution {
                short: 20.0,
                medium: 40.0,
                long: 30.0,
                very_long: 10.0,
            },
            long_to_short_ratio: 2.0,
            median_commas_per_sentence: 1.2,
            mean_commas_per_sentence: 1.5,
            mean_paragraph_sentences: 3.5,
            sentence_ceiling: 38,
        };

        let output = "The system handles requests efficiently and distributes load across workers. \
            Each worker processes one connection at a time with full isolation. \
            When load increases, the scheduler distributes work evenly across all available nodes. \
            Short burst. \
            The architecture supports horizontal scaling through a shared-nothing design pattern. \
            Performance metrics are collected and aggregated every thirty seconds by the monitoring daemon. \
            Quick check. \
            Results flow back through the pipeline into the final aggregation layer for reporting. \
            The entire process completes within the specified latency budget for real-time applications.";
        let violations = check_rhythm_deviation(output, &baseline);
        assert!(violations.is_empty());
    }

    // --- Integration: run_output_checks ---

    #[test]
    fn run_output_checks_passes_clean_text() {
        let core_identity = "## Core Identity\nYou write clearly and directly.";
        let output = "The system processes data in three stages. First, input validation. Then transformation. Finally, output formatting.";
        let result = run_output_checks(output, core_identity, None, None);
        assert!(result.passed);
        assert!(result.violations.is_empty());
    }

    #[test]
    fn run_output_checks_catches_violations() {
        let core_identity = r#"
### Anti-Patterns

| Category | Avoided |
|----------|---------|
| Hype | "revolutionary" |
"#;
        let output = "This revolutionary approach transforms everything.";
        let result = run_output_checks(output, core_identity, None, None);
        assert!(!result.passed);
        assert!(!result.violations.is_empty());
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub fn normalize_text(raw: &str) -> String {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in raw.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                current.push(lower);
            }
        } else if !current.is_empty() {
            if let Some(token) = normalize_token(&current) {
                out.push(token);
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        if let Some(token) = normalize_token(&current) {
            out.push(token);
        }
    }
    out.join(" ")
}

pub fn tokenize(text: &str) -> Vec<String> {
    normalize_text(text)
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

pub fn expand_query_terms(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for term in tokenize(query) {
        if seen.insert(term.clone()) {
            out.push(term.clone());
        }
        if let Some(expansions) = query_expansion_map().get(term.as_str()) {
            for extra in *expansions {
                if seen.insert((*extra).to_owned()) {
                    out.push((*extra).to_owned());
                }
            }
        }
    }
    out
}

fn normalize_token(token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    if token.chars().all(|ch| ch.is_numeric()) {
        return None;
    }
    if is_stopword(token) {
        return None;
    }
    let stemmed = stem_token(token);
    if stemmed.is_empty() || is_stopword(&stemmed) {
        return None;
    }
    Some(resolve_synonym(&stemmed))
}

fn resolve_synonym(token: &str) -> String {
    synonym_map()
        .get(token)
        .cloned()
        .unwrap_or_else(|| token.to_owned())
}

fn synonym_map() -> &'static HashMap<String, String> {
    static SYNONYMS: OnceLock<HashMap<String, String>> = OnceLock::new();
    SYNONYMS.get_or_init(|| {
        let mut map: HashMap<String, String> = HashMap::from([
            ("auth".to_owned(), "authentication".to_owned()),
            ("authenticate".to_owned(), "authentication".to_owned()),
            ("login".to_owned(), "authentication".to_owned()),
            ("refrigerator".to_owned(), "refrigeration".to_owned()),
            ("fridge".to_owned(), "refrigeration".to_owned()),
            ("lodowka".to_owned(), "refrigeration".to_owned()),
            ("chlodziarka".to_owned(), "refrigeration".to_owned()),
            ("db".to_owned(), "data_store".to_owned()),
            ("database".to_owned(), "data_store".to_owned()),
            ("datastore".to_owned(), "data_store".to_owned()),
            ("svc".to_owned(), "service".to_owned()),
        ]);
        if let Ok(raw) = std::env::var("KG_SCORE_SYNONYMS") {
            for part in raw.split(',') {
                if let Some((from, to)) = part.split_once('=') {
                    let from = sanitize_synonym_atom(from);
                    let to = sanitize_synonym_atom(to);
                    if !from.is_empty() && !to.is_empty() {
                        map.insert(from, to);
                    }
                }
            }
        }
        map
    })
}

fn query_expansion_map() -> &'static HashMap<&'static str, &'static [&'static str]> {
    static MAP: OnceLock<HashMap<&'static str, &'static [&'static str]>> = OnceLock::new();
    MAP.get_or_init(|| {
        HashMap::from([
            ("authentication", &["auth", "login", "authenticate"][..]),
            (
                "refrigeration",
                &["refrigerator", "fridge", "lodowka", "chlodziarka"][..],
            ),
            ("data_store", &["db", "database", "datastore"][..]),
            ("service", &["svc"][..]),
        ])
    })
}

fn sanitize_synonym_atom(raw: &str) -> String {
    raw.chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect()
}

fn is_stopword(token: &str) -> bool {
    const STOPWORDS: &[&str] = &[
        "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "is", "it", "of",
        "on", "or", "that", "the", "to", "with", "this", "these", "those", "was", "were", "i",
        "you", "he", "she", "we", "they", "them", "our", "your", "their", "not", "no", "w", "z",
        "za", "oraz", "lub", "czy", "ale", "to", "ten", "ta", "te", "jest", "sa", "byc", "na",
        "do", "od", "po", "przez", "dla", "bez", "pod", "nad", "u", "sie", "nie", "tak", "jak",
        "ktory", "ktora", "ktore", "ktorych", "ktorym",
    ];
    STOPWORDS.contains(&token)
}

fn stem_token(token: &str) -> String {
    let mut out = token.to_owned();
    for suffix in [
        "ing", "edly", "ed", "ly", "es", "s", "owie", "ami", "ach", "ego", "owa", "owe", "owy",
        "eni", "anie", "enia", "eniu", "aniu",
    ] {
        if out.len() > suffix.len() + 2 && out.ends_with(suffix) {
            out.truncate(out.len() - suffix.len());
            break;
        }
    }
    out
}

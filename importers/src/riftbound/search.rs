use serde_json::{json, Value};

pub const LIMIT: usize = 20;

pub fn search_url(site: &str, query: &str, page: u32) -> Result<String, String> {
    if query.len() > 160 || query.chars().any(char::is_control) || !(1..=10).contains(&page) {
        return Err("Search needs at most 160 bytes and page 1–10".into());
    }
    let query = crate::naming::encode_component(query.trim());
    match site {
        "piltover" => Ok(format!(
            "https://piltoverarchive.com/decks?q={query}&page={page}"
        )),
        "riftdecks" => Ok(format!(
            "https://riftdecks.com/riftbound-decks?omni={query}&page={page}"
        )),
        _ => Err("Choose piltover or riftdecks".into()),
    }
}

fn plain(text: &str) -> String {
    let mut inside = false;
    let text: String = text
        .chars()
        .filter(|c| match c {
            '<' => {
                inside = true;
                false
            }
            '>' => {
                inside = false;
                false
            }
            _ => !inside,
        })
        .collect();
    text.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}

pub fn parse_results(site: &str, body: &str) -> Result<Value, String> {
    let origin = match site {
        "piltover" => "https://piltoverarchive.com",
        "riftdecks" => "https://riftdecks.com",
        _ => return Err("Unknown deck site".into()),
    };
    let mut rows = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for chunk in body.split("<a ").skip(1) {
        let Some((attrs, contents)) = chunk.split_once('>') else {
            continue;
        };
        let Some(attr) = attrs.split_once("href=").map(|(_, rest)| rest) else {
            continue;
        };
        let Some(quote @ ('\'' | '"')) = attr.chars().next() else {
            continue;
        };
        let Some(href) = attr[1..].split(quote).next() else {
            continue;
        };
        let path = href.strip_prefix(origin).unwrap_or(href);
        let valid = match site {
            "piltover" => path.starts_with("/decks/view/"),
            _ => path.starts_with("/riftbound-metagame/deck-") || path.starts_with("/decks/view/"),
        };
        if !valid || path.len() > 1000 || path.contains(['\\', '\r', '\n']) {
            continue;
        }
        let title = plain(contents.split("</a>").next().unwrap_or_default());
        if title.is_empty() {
            continue;
        }
        let url = format!("{origin}{path}").replace("&amp;", "&");
        if seen.insert(url.clone()) {
            rows.push(json!({"title": title, "url": url}));
        }
        if rows.len() == LIMIT {
            break;
        }
    }
    Ok(json!({"results": rows}))
}

#[cfg(feature = "riftbound-native")]
pub fn search(site: &str, query: &str, page: u32) -> Result<Value, String> {
    use super::query::{Fetch, UreqFetch};
    let url = search_url(site, query, page)?;
    let body = UreqFetch::new().get(&url)?;
    parse_results(site, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "riftbound-native")]
    #[test]
    #[ignore = "live public website requests"]
    fn live_search_sites_return_named_decks() {
        for site in ["piltover", "riftdecks"] {
            let result = search(site, "Lillia", 1).unwrap();
            let rows = result["results"].as_array().unwrap();
            assert!(!rows.is_empty(), "{site} returned no deck links");
            assert!(rows.len() <= LIMIT);
        }
    }

    #[test]
    fn search_is_fixed_origin_encoded_and_bounded() {
        assert!(search_url("https://evil.example", "", 1).is_err());
        assert!(search_url("piltover", "", 0).is_err());
        assert!(search_url("riftdecks", &"x".repeat(161), 1).is_err());
        assert!(search_url("piltover", "a&b", 1).unwrap().contains("a%26b"));
    }

    #[test]
    fn extracts_only_named_deck_links_and_never_executable_markup() {
        let page = "<a href=\"/decks/view/one\"><span>Example &amp; deck</span></a><a href='/decks/view/one'>duplicate</a><a href='https://evil.example/decks/view/two'>wrong origin</a><a href='javascript:alert(1)'>bad</a>";
        let result = parse_results("piltover", page).unwrap();
        assert_eq!(result["results"].as_array().unwrap().len(), 1);
        assert_eq!(result["results"][0]["title"], "Example & deck");
        let rift = parse_results(
            "riftdecks",
            "<a href='/riftbound-metagame/deck-test-1'>Test</a>",
        )
        .unwrap();
        assert_eq!(
            rift["results"][0]["url"],
            "https://riftdecks.com/riftbound-metagame/deck-test-1"
        );
    }
}

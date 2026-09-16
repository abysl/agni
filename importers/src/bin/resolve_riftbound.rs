use agni_importers::riftbound::catalog::Cached;
use agni_importers::riftbound::ingest::load_catalog;
use agni_importers::riftbound::query::{
    reply_for, resolve_query_deck, DeckQuery, QueryReply, ResolvedQuery, UreqFetch,
};
use agni_importers::riftbound::riftcodex::Riftcodex;
use agni_importers::riftbound::{code_list, deck_code, link, text_list};
use std::path::PathBuf;

fn usage() -> &'static str {
    "usage: resolve-riftbound [--text|--code|--code-list|--link] <url|code|-> [store-dir]\n  - reads a text or code list from stdin\n  --text prints the resolved deck as a text list, --code as a Piltover Archive deck code,\n  --code-list as SET-NNN-COUNT tokens, --link as a deckbuilder URL; the default is JSON"
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Output {
    Json,
    Text,
    Code,
    CodeList,
    Link,
}

fn output_of(flag: &str) -> Option<Output> {
    match flag {
        "--text" => Some(Output::Text),
        "--code" => Some(Output::Code),
        "--code-list" => Some(Output::CodeList),
        "--link" => Some(Output::Link),
        _ => None,
    }
}

fn render(output: Output, resolved: &ResolvedQuery) -> Result<String, String> {
    let deck = &resolved.resolution.deck;
    match output {
        Output::Json => serde_json::to_string_pretty(&reply_for(resolved).body)
            .map_err(|error| error.to_string()),
        Output::Text => Ok(text_list::render(deck)),
        Output::Code => deck_code::encode_deck(deck),
        Output::CodeList => code_list::render(deck),
        Output::Link => deck_code::encode_deck(deck).map(|code| link::piltover_url(&code)),
    }
}

fn main() -> agni_importers::IngestResult<()> {
    let mut args = std::env::args().skip(1).peekable();
    let mut output = Output::Json;
    while let Some(flag) = args.peek().and_then(|arg| output_of(arg)) {
        output = flag;
        args.next();
    }
    let input = args.next().ok_or_else(usage)?;
    let dir = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var("HOME").expect("HOME is not set");
        PathBuf::from(home).join(".spirit/store")
    });

    let query = if input == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
        DeckQuery::Text(text)
    } else if input.contains("://") {
        DeckQuery::Url(input)
    } else {
        DeckQuery::Code(input)
    };

    let mut fetch = UreqFetch::new();
    let resolved: Result<ResolvedQuery, QueryReply> = match load_catalog(&dir)? {
        Some(catalog) => {
            eprintln!(
                "resolving against the local riftbound catalog ({} cards)",
                catalog.len()
            );
            let mut cards = Cached::new(catalog);
            resolve_query_deck(&query, &mut fetch, &mut cards)
        }
        None => {
            eprintln!(
                "no riftbound ref in {}; resolving live against riftcodex",
                dir.display()
            );
            let mut cards: Cached<Riftcodex> = Cached::new(Riftcodex::new());
            resolve_query_deck(&query, &mut fetch, &mut cards)
        }
    };

    let resolved = match resolved {
        Ok(resolved) => resolved,
        Err(reply) => {
            println!("{}", serde_json::to_string_pretty(&reply.body)?);
            return Err(format!("resolve failed with status {}", reply.status).into());
        }
    };
    for entry in &resolved.resolution.unresolved {
        eprintln!("unresolved: {} ({})", entry.identifier, entry.reason);
    }
    println!("{}", render(output, &resolved)?);
    Ok(())
}

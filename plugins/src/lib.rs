use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardCi(pub String);

impl CardCi {
    pub fn parse(text: &str) -> Option<Self> {
        let hash = text.strip_prefix("ci:")?;
        if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(Self(text.to_string()))
        } else {
            None
        }
    }
}

impl std::fmt::Display for CardCi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub trait CardScript: Send + Sync {
    fn binds_to(&self) -> CardCi;
    fn rules_record(&self) -> Option<String> {
        None
    }
}

#[derive(Default)]
pub struct ScriptRegistry {
    scripts: BTreeMap<CardCi, Arc<dyn CardScript>>,
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, script: Arc<dyn CardScript>) -> Result<(), BindError> {
        let card = script.binds_to();
        if self.scripts.contains_key(&card) {
            return Err(BindError::AlreadyBound(card));
        }
        self.scripts.insert(card, script);
        Ok(())
    }

    pub fn lookup(&self, card: &CardCi) -> Option<&Arc<dyn CardScript>> {
        self.scripts.get(card)
    }

    pub fn bound(&self) -> impl Iterator<Item = &CardCi> {
        self.scripts.keys()
    }

    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindError {
    AlreadyBound(CardCi),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::AlreadyBound(card) => write!(f, "a script is already bound to {card}"),
        }
    }
}

impl std::error::Error for BindError {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Bolt;

    impl CardScript for Bolt {
        fn binds_to(&self) -> CardCi {
            CardCi(format!("ci:{}", "ab".repeat(32)))
        }
    }

    #[test]
    fn a_ci_is_a_prefixed_blake3_hex_hash() {
        assert!(CardCi::parse(&format!("ci:{}", "ab".repeat(32))).is_some());
        assert!(CardCi::parse("ci:short").is_none());
        assert!(CardCi::parse(&"ab".repeat(32)).is_none());
    }

    #[test]
    fn a_script_binds_to_exactly_one_identity() {
        let mut registry = ScriptRegistry::new();
        assert!(registry.bind(Arc::new(Bolt)).is_ok());
        assert_eq!(
            registry.bind(Arc::new(Bolt)),
            Err(BindError::AlreadyBound(Bolt.binds_to()))
        );
        assert_eq!(registry.len(), 1);
        assert!(registry.lookup(&Bolt.binds_to()).is_some());
    }

    #[test]
    fn bound_identities_iterate_in_deterministic_order() {
        struct At(String);
        impl CardScript for At {
            fn binds_to(&self) -> CardCi {
                CardCi(self.0.clone())
            }
        }
        let mut registry = ScriptRegistry::new();
        for tag in ["cc", "aa", "bb"] {
            registry
                .bind(Arc::new(At(format!("ci:{}", tag.repeat(32)))))
                .unwrap();
        }
        let order: Vec<String> = registry.bound().map(|ci| ci.0.clone()).collect();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted);
    }
}

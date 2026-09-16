pub fn normalize_name(name: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(c.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

pub fn encode_component(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_components_encode_the_awkward_names() {
        assert_eq!(
            encode_component("Serelith, Tidebound Oracle"),
            "Serelith%2C%20Tidebound%20Oracle"
        );
        assert_eq!(encode_component("unl-229*"), "unl-229%2A");
        assert_eq!(encode_component("Thornspire"), "Thornspire");
        assert_eq!(
            encode_component("3 Emberwing Scout\n"),
            "3%20Emberwing%20Scout%0A"
        );
    }

    #[test]
    fn punctuation_and_case_normalize_away() {
        assert_eq!(
            normalize_name("Serelith, Tidebound-Oracle!"),
            "serelith tidebound oracle"
        );
        assert_eq!(normalize_name("  spaced   out  "), "spaced out");
        assert_eq!(normalize_name(""), "");
    }
}

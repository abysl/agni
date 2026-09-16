use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetCode {
    Ogn,
    Ogs,
    Arc,
    Sfd,
    Unl,
    Ven,
    Rad,
    Opp,
    Pr,
    Jdg,
}

pub const UNPUBLISHED_WIRE: u8 = u8::MAX;

impl SetCode {
    pub const ALL: [Self; 10] = [
        Self::Ogn,
        Self::Ogs,
        Self::Arc,
        Self::Sfd,
        Self::Unl,
        Self::Ven,
        Self::Rad,
        Self::Opp,
        Self::Pr,
        Self::Jdg,
    ];

    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Ogn),
            1 => Some(Self::Ogs),
            2 => Some(Self::Arc),
            3 => Some(Self::Sfd),
            4 => Some(Self::Unl),
            5 => Some(Self::Ven),
            6 => Some(Self::Rad),
            _ => None,
        }
    }

    pub fn wire(self) -> Option<u8> {
        match self {
            Self::Ogn => Some(0),
            Self::Ogs => Some(1),
            Self::Arc => Some(2),
            Self::Sfd => Some(3),
            Self::Unl => Some(4),
            Self::Ven => Some(5),
            Self::Rad => Some(6),
            Self::Opp | Self::Pr | Self::Jdg => None,
        }
    }

    pub fn to_wire(self) -> u8 {
        self.wire().unwrap_or(UNPUBLISHED_WIRE)
    }

    pub fn reprints_another_set(self) -> bool {
        matches!(self, Self::Opp | Self::Pr | Self::Jdg)
    }

    pub fn base_for_total(total: u32) -> Option<Self> {
        match total {
            298 => Some(Self::Ogn),
            24 => Some(Self::Ogs),
            221 => Some(Self::Sfd),
            219 => Some(Self::Unl),
            166 => Some(Self::Ven),
            _ => None,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_uppercase().as_str() {
            "OGN" => Some(Self::Ogn),
            "OGS" => Some(Self::Ogs),
            "ARC" => Some(Self::Arc),
            "SFD" => Some(Self::Sfd),
            "UNL" => Some(Self::Unl),
            "VEN" => Some(Self::Ven),
            "RAD" => Some(Self::Rad),
            "OPP" => Some(Self::Opp),
            "PR" => Some(Self::Pr),
            "JDG" => Some(Self::Jdg),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ogn => "OGN",
            Self::Ogs => "OGS",
            Self::Arc => "ARC",
            Self::Sfd => "SFD",
            Self::Unl => "UNL",
            Self::Ven => "VEN",
            Self::Rad => "RAD",
            Self::Opp => "OPP",
            Self::Pr => "PR",
            Self::Jdg => "JDG",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CardNumber {
    Normal(u32),
    Rune(u32),
    Special(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variant {
    Base,
    Alternate,
    Signed,
    B,
}

impl Variant {
    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Base),
            1 => Some(Self::Alternate),
            2 => Some(Self::Signed),
            3 => Some(Self::B),
            _ => None,
        }
    }

    pub fn to_wire(self) -> u8 {
        match self {
            Self::Base => 0,
            Self::Alternate => 1,
            Self::Signed => 2,
            Self::B => 3,
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::Base => "",
            Self::Alternate => "a",
            Self::Signed => "s",
            Self::B => "b",
        }
    }

    pub fn id_suffix(self) -> &'static str {
        match self {
            Self::Signed => "*",
            other => other.suffix(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardCode {
    pub set: SetCode,
    pub number: CardNumber,
    pub variant: Variant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardCodeError {
    pub input: String,
}

impl fmt::Display for CardCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not a card code like OGN-045, OGN-R05, OGN-SP3 or UNL-116a",
            self.input
        )
    }
}

impl std::error::Error for CardCodeError {}

impl CardCode {
    pub fn parse(text: &str) -> Result<Self, CardCodeError> {
        let error = || CardCodeError { input: text.into() };
        let (set_text, rest) = text.split_once('-').ok_or_else(error)?;
        let set = SetCode::parse(set_text).ok_or_else(error)?;
        let rest_upper = rest.to_ascii_uppercase();
        let (kind, digits_at) = if rest_upper.starts_with("SP") {
            (1, 2)
        } else if rest_upper.starts_with('R') {
            (2, 1)
        } else {
            (0, 0)
        };
        let tail = &rest[digits_at..];
        let digits_len = tail.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len == 0 {
            return Err(error());
        }
        let value: u32 = tail[..digits_len].parse().map_err(|_| error())?;
        let number = match kind {
            1 => CardNumber::Special(value),
            2 => CardNumber::Rune(value),
            _ => CardNumber::Normal(value),
        };
        let variant = match &tail[digits_len..] {
            "" => Variant::Base,
            "a" => Variant::Alternate,
            "s" | "*" => Variant::Signed,
            "b" => Variant::B,
            _ => return Err(error()),
        };
        Ok(Self {
            set,
            number,
            variant,
        })
    }

    pub fn from_riftbound_id(id: &str) -> Result<Self, CardCodeError> {
        let error = || CardCodeError { input: id.into() };
        let (set_text, rest) = id.split_once('-').ok_or_else(error)?;
        let set = SetCode::parse(set_text).ok_or_else(error)?;
        let body = match rest.rsplit_once('-') {
            Some((body, size)) if size.chars().all(|c| c.is_ascii_digit()) && !body.is_empty() => {
                body
            }
            _ => rest,
        };
        let code = Self::parse(&format!("{}-{body}", set.as_str()))?;
        Ok(code)
    }

    pub fn base_print(self, total: Option<u32>) -> Self {
        if !self.set.reprints_another_set() {
            return self;
        }
        match total.and_then(SetCode::base_for_total) {
            Some(set) => Self { set, ..self },
            None => self,
        }
    }

    pub fn base_print_of_id(id: &str) -> Result<Self, CardCodeError> {
        Ok(Self::from_riftbound_id(id)?.base_print(set_total(id)))
    }

    pub fn number_text(&self) -> String {
        match self.number {
            CardNumber::Normal(value) => format!("{value:03}"),
            CardNumber::Rune(value) => format!("R{value:02}"),
            CardNumber::Special(value) => format!("SP{value}"),
        }
    }

    pub fn id_fragment(&self) -> String {
        format!(
            "{}-{}{}",
            self.set.as_str().to_ascii_lowercase(),
            self.number_text().to_ascii_lowercase(),
            self.variant.id_suffix()
        )
    }
}

pub fn set_total(id: &str) -> Option<u32> {
    let (_, rest) = id.split_once('-')?;
    let (body, total) = rest.rsplit_once('-')?;
    if body.is_empty() || total.is_empty() || !total.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    total.parse().ok()
}

impl fmt::Display for CardCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}{}",
            self.set.as_str(),
            self.number_text(),
            self.variant.suffix()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_codes_round_trip_through_display() {
        for text in ["OGN-045", "OGN-R05", "UNL-SP3", "UNL-116a", "SFD-201b"] {
            let code = CardCode::parse(text).unwrap();
            assert_eq!(code.to_string(), text);
        }
    }

    #[test]
    fn signed_codes_accept_both_suffixes() {
        let star = CardCode::parse("UNL-229*").unwrap();
        let s = CardCode::parse("UNL-229s").unwrap();
        assert_eq!(star, s);
        assert_eq!(star.to_string(), "UNL-229s");
        assert_eq!(star.id_fragment(), "unl-229*");
    }

    #[test]
    fn numbers_are_padded_like_the_reference() {
        let normal = CardCode::parse("OGN-7").unwrap();
        assert_eq!(normal.to_string(), "OGN-007");
        let rune = CardCode::parse("VEN-R1").unwrap();
        assert_eq!(rune.to_string(), "VEN-R01");
        let special = CardCode::parse("UNL-SP12").unwrap();
        assert_eq!(special.to_string(), "UNL-SP12");
    }

    #[test]
    fn malformed_codes_are_refused() {
        for text in ["OGN045", "XYZ-045", "OGN-", "OGN-abc", "OGN-045z", "OGN-R"] {
            assert!(CardCode::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn riftbound_ids_parse_with_and_without_a_set_size() {
        let with_size = CardCode::from_riftbound_id("ogn-045-298").unwrap();
        assert_eq!(with_size.to_string(), "OGN-045");
        let rune = CardCode::from_riftbound_id("ven-r01").unwrap();
        assert_eq!(rune.to_string(), "VEN-R01");
        let signed = CardCode::from_riftbound_id("unl-229*-219").unwrap();
        assert_eq!(signed.to_string(), "UNL-229s");
        let alternate = CardCode::from_riftbound_id("unl-116a-219").unwrap();
        assert_eq!(alternate.to_string(), "UNL-116a");
    }

    #[test]
    fn every_catalog_set_parses_and_prints_itself_back() {
        for set in SetCode::ALL {
            assert_eq!(SetCode::parse(set.as_str()), Some(set));
            assert_eq!(
                SetCode::parse(&set.as_str().to_ascii_lowercase()),
                Some(set)
            );
            match set.wire() {
                Some(wire) => assert_eq!(SetCode::from_wire(wire), Some(set)),
                None => assert!(set.reprints_another_set(), "{set:?}"),
            }
        }
        assert_eq!(SetCode::from_wire(UNPUBLISHED_WIRE), None);
        assert_eq!(SetCode::Opp.to_wire(), UNPUBLISHED_WIRE);
    }

    #[test]
    fn opp_codes_round_trip_through_every_spelling() {
        for text in [
            "OPP-083", "OPP-SP1", "OPP-R01", "OPP-118a", "PR-246b", "JDG-087",
        ] {
            let code = CardCode::parse(text).unwrap();
            assert_eq!(code.to_string(), text);
            assert_eq!(CardCode::parse(&code.to_string()).unwrap(), code);
        }
        let opp = CardCode::from_riftbound_id("opp-083-298").unwrap();
        assert_eq!(opp.set, SetCode::Opp);
        assert_eq!(opp.to_string(), "OPP-083");
        assert_eq!(opp.id_fragment(), "opp-083");
        assert_eq!(CardCode::from_riftbound_id("OPP-083").unwrap(), opp);
        assert_eq!(CardCode::from_riftbound_id("opp-083").unwrap(), opp);
        assert_eq!(
            CardCode::from_riftbound_id("opp-sp1").unwrap().number,
            CardNumber::Special(1)
        );
    }

    #[test]
    fn reprint_sets_fold_to_the_base_print_their_total_names() {
        assert_eq!(
            CardCode::base_print_of_id("opp-083-298")
                .unwrap()
                .to_string(),
            "OGN-083"
        );
        assert_eq!(
            CardCode::base_print_of_id("opp-019-024")
                .unwrap()
                .to_string(),
            "OGS-019"
        );
        assert_eq!(
            CardCode::base_print_of_id("opp-118a-221")
                .unwrap()
                .to_string(),
            "SFD-118a"
        );
        assert_eq!(
            CardCode::base_print_of_id("opp-058-219")
                .unwrap()
                .to_string(),
            "UNL-058"
        );
        assert_eq!(
            CardCode::base_print_of_id("pr-202-298")
                .unwrap()
                .to_string(),
            "OGN-202"
        );
        assert_eq!(
            CardCode::base_print_of_id("jdg-087-219")
                .unwrap()
                .to_string(),
            "UNL-087"
        );
        let bare = CardCode::parse("OPP-083").unwrap();
        assert_eq!(bare.base_print(None), bare);
        assert_eq!(bare.base_print(Some(7)), bare);
        let already_base = CardCode::from_riftbound_id("ven-192-166").unwrap();
        assert_eq!(already_base.base_print(Some(298)), already_base);
        assert_eq!(set_total("opp-083-298"), Some(298));
        assert_eq!(set_total("unl-229*-219"), Some(219));
        assert_eq!(set_total("ven-r01"), None);
        assert_eq!(set_total("OPP-083"), None);
    }

    #[test]
    fn ordering_matches_the_reference_grouping_sort() {
        let mut numbers = vec![
            CardNumber::Special(3),
            CardNumber::Rune(5),
            CardNumber::Normal(1000),
            CardNumber::Normal(7),
        ];
        numbers.sort();
        assert_eq!(
            numbers,
            vec![
                CardNumber::Normal(7),
                CardNumber::Normal(1000),
                CardNumber::Rune(5),
                CardNumber::Special(3),
            ]
        );
    }
}

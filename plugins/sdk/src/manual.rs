pub const DISABLE: &str = "disable rules enforcement";
pub const CONTROLS: &str = "manual controls";
const PREFIX: &[u8] = b"manual\x01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Disable,
    Look { zone: u16, count: u32 },
    Shuffle { zone: u16, seed: u64 },
    Turn { seat: u8, number: u16 },
    Control { zone: u16, seat: Option<u8> },
    RemoveToken { card: u32 },
    Reveal { card: u32 },
    Conceal { card: u32 },
}

impl Command {
    pub fn recognizes(bytes: &[u8]) -> bool {
        bytes.starts_with(PREFIX)
    }

    pub fn encode(self) -> Vec<u8> {
        let mut out = PREFIX.to_vec();
        match self {
            Self::Disable => out.push(0),
            Self::Look { zone, count } => {
                out.push(1);
                out.extend(zone.to_le_bytes());
                out.extend(count.to_le_bytes());
            }
            Self::Shuffle { zone, seed } => {
                out.push(2);
                out.extend(zone.to_le_bytes());
                out.extend(seed.to_le_bytes());
            }
            Self::Turn { seat, number } => {
                out.extend([3, seat]);
                out.extend(number.to_le_bytes());
            }
            Self::Control { zone, seat } => {
                out.push(4);
                out.extend(zone.to_le_bytes());
                out.push(seat.unwrap_or(u8::MAX));
            }
            Self::RemoveToken { card } => {
                out.push(5);
                out.extend(card.to_le_bytes());
            }
            Self::Reveal { card } => {
                out.push(6);
                out.extend(card.to_le_bytes());
            }
            Self::Conceal { card } => {
                out.push(7);
                out.extend(card.to_le_bytes());
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        match bytes.strip_prefix(PREFIX)? {
            [0] => Some(Self::Disable),
            [1, a, b, count @ ..] if count.len() == 4 => Some(Self::Look {
                zone: u16::from_le_bytes([*a, *b]),
                count: u32::from_le_bytes(count.try_into().ok()?),
            }),
            [2, a, b, seed @ ..] if seed.len() == 8 => Some(Self::Shuffle {
                zone: u16::from_le_bytes([*a, *b]),
                seed: u64::from_le_bytes(seed.try_into().ok()?),
            }),
            [3, seat, a, b] => Some(Self::Turn {
                seat: *seat,
                number: u16::from_le_bytes([*a, *b]),
            }),
            [4, a, b, seat] => Some(Self::Control {
                zone: u16::from_le_bytes([*a, *b]),
                seat: (*seat != u8::MAX).then_some(*seat),
            }),
            [5, a, b, c, d] => Some(Self::RemoveToken {
                card: u32::from_le_bytes([*a, *b, *c, *d]),
            }),
            [6, a, b, c, d] => Some(Self::Reveal {
                card: u32::from_le_bytes([*a, *b, *c, *d]),
            }),
            [7, a, b, c, d] => Some(Self::Conceal {
                card: u32::from_le_bytes([*a, *b, *c, *d]),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_roundtrip_and_reject_trailing_or_missing_bytes() {
        for command in [
            Command::Disable,
            Command::Look {
                zone: 258,
                count: 1000,
            },
            Command::Shuffle {
                zone: 2,
                seed: u64::MAX,
            },
            Command::Turn {
                seat: 3,
                number: 500,
            },
            Command::Control {
                zone: 9,
                seat: None,
            },
            Command::Control {
                zone: 10,
                seat: Some(1),
            },
            Command::RemoveToken { card: 1000 },
            Command::Reveal { card: 42 },
            Command::Conceal { card: 42 },
        ] {
            let mut bytes = command.encode();
            assert_eq!(Command::decode(&bytes), Some(command));
            assert!(Command::decode(&bytes[..bytes.len() - 1]).is_none());
            bytes.push(0);
            assert!(Command::decode(&bytes).is_none());
        }
    }
}

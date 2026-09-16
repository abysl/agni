pub const SECRET_LEN: usize = 8;
pub const PER_SEAT: usize = 2 + 2 * SECRET_LEN;

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

pub fn commitment(secret: &[u8; SECRET_LEN]) -> [u8; SECRET_LEN] {
    let mut acc: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in secret {
        acc ^= *byte as u64;
        acc = acc.wrapping_mul(0x0000_0100_0000_01b3);
        acc = mix(acc);
    }
    mix(acc ^ 0x5eed_d1ce_0000_0001).to_le_bytes()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hand {
    pub commit: Option<[u8; SECRET_LEN]>,
    pub secret: Option<[u8; SECRET_LEN]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roll {
    pub sides: u8,
    pub round: u8,
    pub hands: Vec<Hand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiceRefusal {
    NoSuchSeat,
    AlreadyCommitted,
    NotEveryoneCommitted,
    NotCommitted,
    AlreadyRevealed,
    WrongSecret,
}

impl DiceRefusal {
    pub fn label(self) -> &'static str {
        match self {
            DiceRefusal::NoSuchSeat => "no such seat",
            DiceRefusal::AlreadyCommitted => "you already rolled",
            DiceRefusal::NotEveryoneCommitted => "waiting for every seat to roll",
            DiceRefusal::NotCommitted => "you have not rolled",
            DiceRefusal::AlreadyRevealed => "you already revealed",
            DiceRefusal::WrongSecret => "the reveal does not match the commitment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Pending,
    Tie(Vec<u8>),
    Winner(u8),
}

impl Roll {
    pub fn new(players: u8, sides: u8) -> Self {
        Self {
            sides: sides.max(2),
            round: 1,
            hands: vec![Hand::default(); usize::from(players.max(1))],
        }
    }

    pub fn players(&self) -> u8 {
        self.hands.len() as u8
    }

    fn hand(&mut self, seat: u8) -> Result<&mut Hand, DiceRefusal> {
        self.hands
            .get_mut(usize::from(seat))
            .ok_or(DiceRefusal::NoSuchSeat)
    }

    pub fn commit(&mut self, seat: u8, commit: [u8; SECRET_LEN]) -> Result<(), DiceRefusal> {
        let hand = self.hand(seat)?;
        if hand.commit.is_some() {
            return Err(DiceRefusal::AlreadyCommitted);
        }
        hand.commit = Some(commit);
        Ok(())
    }

    pub fn all_committed(&self) -> bool {
        self.hands.iter().all(|hand| hand.commit.is_some())
    }

    pub fn all_revealed(&self) -> bool {
        self.hands.iter().all(|hand| hand.secret.is_some())
    }

    pub fn reveal(&mut self, seat: u8, secret: [u8; SECRET_LEN]) -> Result<(), DiceRefusal> {
        if !self.all_committed() {
            return Err(DiceRefusal::NotEveryoneCommitted);
        }
        let hand = self.hand(seat)?;
        let Some(commit) = hand.commit else {
            return Err(DiceRefusal::NotCommitted);
        };
        if hand.secret.is_some() {
            return Err(DiceRefusal::AlreadyRevealed);
        }
        if commitment(&secret) != commit {
            return Err(DiceRefusal::WrongSecret);
        }
        hand.secret = Some(secret);
        Ok(())
    }

    pub fn pooled(&self) -> Option<u64> {
        let mut pool: u64 = 0;
        for hand in &self.hands {
            pool ^= mix(u64::from_le_bytes(hand.secret?));
        }
        Some(pool)
    }

    pub fn die(&self, seat: u8) -> Option<u8> {
        let pool = self.pooled()?;
        let sides = u64::from(self.sides);
        Some((mix(pool ^ (u64::from(seat) << 56) ^ u64::from(self.round)) % sides) as u8 + 1)
    }

    pub fn dice(&self) -> Vec<Option<u8>> {
        (0..self.players()).map(|seat| self.die(seat)).collect()
    }

    pub fn outcome(&self) -> Outcome {
        let dice: Vec<u8> = match self.dice().into_iter().collect::<Option<Vec<u8>>>() {
            Some(dice) => dice,
            None => return Outcome::Pending,
        };
        let best = dice.iter().copied().max().unwrap_or(0);
        let leaders: Vec<u8> = dice
            .iter()
            .enumerate()
            .filter(|(_, die)| **die == best)
            .map(|(seat, _)| seat as u8)
            .collect();
        match leaders.as_slice() {
            [winner] => Outcome::Winner(*winner),
            _ => Outcome::Tie(leaders),
        }
    }

    pub fn again(&self) -> Self {
        Self {
            sides: self.sides,
            round: self.round.saturating_add(1),
            hands: vec![Hand::default(); self.hands.len()],
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.sides, self.round, self.players()];
        for hand in &self.hands {
            for slot in [hand.commit, hand.secret] {
                match slot {
                    Some(bytes) => {
                        out.push(1);
                        out.extend(bytes);
                    }
                    None => {
                        out.push(0);
                        out.extend([0; SECRET_LEN]);
                    }
                }
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (&[sides, round, players], rest) = bytes.split_at_checked(3)? else {
            return None;
        };
        if rest.len() != usize::from(players) * PER_SEAT {
            return None;
        }
        let mut hands = Vec::with_capacity(usize::from(players));
        for chunk in rest.as_chunks::<PER_SEAT>().0 {
            let slot = |offset: usize| -> Option<Option<[u8; SECRET_LEN]>> {
                let flag = chunk[offset];
                let bytes: [u8; SECRET_LEN] =
                    chunk[offset + 1..offset + 1 + SECRET_LEN].try_into().ok()?;
                match flag {
                    0 => Some(None),
                    1 => Some(Some(bytes)),
                    _ => None,
                }
            };
            hands.push(Hand {
                commit: slot(0)?,
                secret: slot(1 + SECRET_LEN)?,
            });
        }
        Some(Self {
            sides,
            round,
            hands,
        })
    }

    pub fn encoded_len(players: u8) -> usize {
        3 + usize::from(players) * PER_SEAT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(seed: u8) -> [u8; SECRET_LEN] {
        [seed; SECRET_LEN]
    }

    #[test]
    fn a_roll_needs_every_commitment_before_any_reveal_and_checks_each_one() {
        let mut roll = Roll::new(2, 6);
        roll.commit(0, commitment(&secret(1))).unwrap();
        assert_eq!(
            roll.commit(0, commitment(&secret(1))),
            Err(DiceRefusal::AlreadyCommitted)
        );
        assert_eq!(
            roll.reveal(0, secret(1)),
            Err(DiceRefusal::NotEveryoneCommitted)
        );
        roll.commit(1, commitment(&secret(2))).unwrap();
        assert_eq!(roll.reveal(1, secret(9)), Err(DiceRefusal::WrongSecret));
        assert_eq!(roll.outcome(), Outcome::Pending);
        roll.reveal(0, secret(1)).unwrap();
        assert_eq!(roll.reveal(0, secret(1)), Err(DiceRefusal::AlreadyRevealed));
        roll.reveal(1, secret(2)).unwrap();
        assert!(roll.all_revealed());
        assert_eq!(
            roll.commit(2, commitment(&secret(3))),
            Err(DiceRefusal::NoSuchSeat)
        );
        let dice = roll.dice();
        assert!(dice.iter().all(|die| (1..=6).contains(&die.unwrap())));
        assert!(matches!(
            roll.outcome(),
            Outcome::Winner(_) | Outcome::Tie(_)
        ));
    }

    #[test]
    fn the_dice_are_a_pure_function_of_the_secrets_and_the_round() {
        let mut a = Roll::new(3, 20);
        let mut b = Roll::new(3, 20);
        for seat in 0..3u8 {
            a.commit(seat, commitment(&secret(seat + 10))).unwrap();
            b.commit(seat, commitment(&secret(seat + 10))).unwrap();
        }
        for seat in 0..3u8 {
            a.reveal(seat, secret(seat + 10)).unwrap();
            b.reveal(seat, secret(seat + 10)).unwrap();
        }
        assert_eq!(a.dice(), b.dice());
        let again = a.again();
        assert_eq!(again.round, 2);
        assert!(again.hands.iter().all(|hand| hand.commit.is_none()));
    }

    #[test]
    fn the_roll_round_trips_through_bytes_at_every_stage() {
        let mut roll = Roll::new(2, 6);
        assert_eq!(Roll::decode(&roll.encode()), Some(roll.clone()));
        roll.commit(1, commitment(&secret(4))).unwrap();
        assert_eq!(Roll::decode(&roll.encode()), Some(roll.clone()));
        assert_eq!(roll.encode().len(), Roll::encoded_len(2));
        assert_eq!(Roll::decode(&roll.encode()[..5]), None);
        assert_eq!(Roll::decode(&[]), None);
    }

    #[test]
    fn a_tie_names_every_leader() {
        let dice_of = |round: u8| {
            let mut roll = Roll::new(2, 2);
            roll.round = round;
            roll.commit(0, commitment(&secret(1))).unwrap();
            roll.commit(1, commitment(&secret(2))).unwrap();
            roll.reveal(0, secret(1)).unwrap();
            roll.reveal(1, secret(2)).unwrap();
            (roll.dice(), roll.outcome())
        };
        let tied = (1..40u8)
            .map(dice_of)
            .find(|(dice, _)| dice[0] == dice[1])
            .expect("some round ties on a coin");
        assert_eq!(tied.1, Outcome::Tie(vec![0, 1]));
        let decided = (1..40u8)
            .map(dice_of)
            .find(|(dice, _)| dice[0] != dice[1])
            .expect("some round decides");
        assert!(matches!(decided.1, Outcome::Winner(_)));
    }
}

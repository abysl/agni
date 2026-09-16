#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnOrder {
    pub players: u8,
    pub turn: u16,
    pub turn_player: u8,
}

impl TurnOrder {
    pub fn start(players: u8, first_player: u8) -> Self {
        Self {
            players: players.max(1),
            turn: 1,
            turn_player: first_player,
        }
    }

    pub fn next_seat(&self, seat: u8) -> u8 {
        (seat + 1) % self.players.max(1)
    }

    pub fn is_turn_player(&self, seat: u8) -> bool {
        self.turn_player == seat
    }

    pub fn advance(&mut self) {
        self.turn = self.turn.saturating_add(1);
        self.turn_player = self.next_seat(self.turn_player);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassWindow {
    pub focus: u8,
    pub passes: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Open,
    Closed,
}

impl PassWindow {
    pub fn open(focus: u8) -> Self {
        Self { focus, passes: 0 }
    }

    pub fn has_focus(&self, seat: u8) -> bool {
        self.focus == seat
    }

    pub fn pass(&mut self, order: &TurnOrder) -> Window {
        self.passes = self.passes.saturating_add(1);
        if self.passes >= order.players {
            return Window::Closed;
        }
        self.focus = order.next_seat(self.focus);
        Window::Open
    }

    pub fn play(&mut self, order: &TurnOrder) {
        self.passes = 0;
        self.focus = order.next_seat(self.focus);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_turn_walks_the_seats_in_order_and_wraps() {
        let mut order = TurnOrder::start(3, 2);
        assert!(order.is_turn_player(2));
        order.advance();
        assert_eq!((order.turn, order.turn_player), (2, 0));
        order.advance();
        order.advance();
        assert_eq!((order.turn, order.turn_player), (4, 2));
        assert_eq!(TurnOrder::start(0, 0).players, 1);
    }

    #[test]
    fn a_window_closes_once_every_seat_has_passed_in_sequence() {
        let order = TurnOrder::start(2, 0);
        let mut window = PassWindow::open(0);
        assert_eq!(window.pass(&order), Window::Open);
        assert!(window.has_focus(1));
        window.play(&order);
        assert_eq!((window.focus, window.passes), (0, 0));
        assert_eq!(window.pass(&order), Window::Open);
        assert_eq!(window.pass(&order), Window::Closed);
    }
}

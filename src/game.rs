use std::ops::Not;

mod searcher;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, bincode::Encode, bincode::Decode)]
pub enum Mark {
    X,
    O,
}

pub const HUMAN_MARK: Mark = Mark::X;
pub const COMPUTER_MARK: Mark = Mark::O;

impl Not for Mark {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Mark::X => Mark::O,
            Mark::O => Mark::X,
        }
    }
}

impl std::fmt::Display for Mark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mark::X => f.pad("X"),
            Mark::O => f.pad("O"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Draw;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Hash, bincode::Encode, bincode::Decode)]
pub struct InnerBoard {
    pub squares: [[Option<Mark>; 3]; 3],
    pub winner: Option<Mark>,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Hash, bincode::Encode, bincode::Decode)]
pub struct OuterBoard {
    pub boards: [[InnerBoard; 3]; 3],
    pub overall_winner: Option<Mark>,
    pub active_square: Option<(u8, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, bincode::Encode, bincode::Decode)]
pub struct Move {
    pub outer: (u8, u8),
    pub inner: (u8, u8),
    pub player: Mark,
}

impl InnerBoard {
    fn update_winner(&mut self) {
        if self.winner.is_some() {
            return;
        }

        for row in 0..3 {
            if let Some(player) = self.squares[row][0] {
                if self.squares[row][1] == Some(player) && self.squares[row][2] == Some(player) {
                    self.winner = Some(player);
                    return;
                }
            }
        }

        for col in 0..3 {
            if let Some(player) = self.squares[0][col] {
                if self.squares[1][col] == Some(player) && self.squares[2][col] == Some(player) {
                    self.winner = Some(player);
                    return;
                }
            }
        }

        if let Some(player) = self.squares[1][1] {
            if (self.squares[0][0] == Some(player) && self.squares[2][2] == Some(player))
                || (self.squares[0][2] == Some(player) && self.squares[2][0] == Some(player))
            {
                self.winner = Some(player);
                return;
            }
        }
    }

    fn can_play(&self) -> bool {
        self.winner.is_none()
            && self
                .squares
                .iter()
                .any(|row| row.iter().any(|&cell| cell.is_none()))
    }

    fn possible_moves(&self) -> Vec<(u8, u8)> {
        let mut moves = Vec::new();
        if !self.can_play() {
            return moves;
        }
        for row in 0..3 {
            for col in 0..3 {
                if self.squares[row][col].is_none() {
                    moves.push((row as u8, col as u8));
                }
            }
        }
        moves
    }
}

impl OuterBoard {
    pub fn random(fill_percentage: f64) -> Self {
        use rand::prelude::*;
        let mut rng = rand::rng();
        let mut this = OuterBoard::default();
        this.active_square = Some((rng.random_range(0..3), rng.random_range(0..3)));
        for row in 0..3 {
            for col in 0..3 {
                for inner_row in 0..3 {
                    for inner_col in 0..3 {
                        if rng.random_bool(fill_percentage) {
                            this.boards[row][col].squares[inner_row][inner_col] =
                                Some(if rng.random_bool(0.5) {
                                    Mark::X
                                } else {
                                    Mark::O
                                });
                        }
                    }
                }
            }
        }

        for row in 0..3 {
            for col in 0..3 {
                this.boards[row][col].update_winner();
            }
        }
        this.update_overall_winner();

        this
    }

    #[must_use]
    pub fn make_move(&self, r#move: Move) -> Option<Self> {
        if self.active_square.is_some_and(|sq| r#move.outer != sq) {
            return None;
        }
        if self.overall_winner.is_some() {
            return None;
        }
        if self.boards[r#move.outer.0 as usize][r#move.outer.1 as usize]
            .winner
            .is_some()
        {
            return None;
        }

        let mut new_self = *self;

        let cell = &mut new_self.boards[r#move.outer.0 as usize][r#move.outer.1 as usize].squares
            [r#move.inner.0 as usize][r#move.inner.1 as usize];
        if cell.is_some() {
            return None;
        }
        *cell = Some(r#move.player);

        new_self.boards[r#move.outer.0 as usize][r#move.outer.1 as usize].update_winner();
        new_self.active_square =
            Some(r#move.inner).filter(|&(r, c)| new_self.boards[r as usize][c as usize].can_play());
        new_self.update_overall_winner();

        Some(new_self)
    }

    fn update_overall_winner(&mut self) {
        if self.overall_winner.is_some() {
            return;
        }

        self.overall_winner = self.meta_board().winner;
    }

    pub fn possible_moves(&self, player: Mark) -> Vec<Move> {
        let mut moves = Vec::new();
        if let Some((outer_row, outer_col)) = self.active_square {
            let inner_board = &self.boards[outer_row as usize][outer_col as usize];
            for (inner_row, inner_col) in inner_board.possible_moves() {
                moves.push(Move {
                    outer: (outer_row, outer_col),
                    inner: (inner_row, inner_col),
                    player,
                });
            }
        } else {
            for outer_row in 0..3 {
                for outer_col in 0..3 {
                    let inner_board = &self.boards[outer_row][outer_col];
                    for (inner_row, inner_col) in inner_board.possible_moves() {
                        moves.push(Move {
                            outer: (outer_row as u8, outer_col as u8),
                            inner: (inner_row, inner_col),
                            player,
                        });
                    }
                }
            }
        }
        moves
    }

    pub fn best_move(&self, player: Mark) -> Option<(Move, i32)> {
        searcher::Searcher::search(self, player)
    }

    fn meta_board(&self) -> InnerBoard {
        let mut meta = InnerBoard::default();
        for row in 0..3 {
            for col in 0..3 {
                meta.squares[row][col] = self.boards[row][col].winner;
            }
        }
        meta.update_winner();
        meta
    }

    fn meta_board_with_draws(&self) -> [[Option<Result<Mark, Draw>>; 3]; 3] {
        let mut meta: [[Option<Result<Mark, Draw>>; 3]; 3] = Default::default();
        for row in 0..3 {
            for col in 0..3 {
                meta[row][col] = self.boards[row][col].winner.map(Ok).or_else(|| {
                    if self.boards[row][col].can_play() {
                        None
                    } else {
                        Some(Err(Draw))
                    }
                });
            }
        }
        meta
    }
}

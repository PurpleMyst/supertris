use std::ops::Not;

use rayon::prelude::*;
use tracing::debug;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Mark {
    #[default]
    X,
    O,
}

pub const HUMAN_MARK: Mark = Mark::X;
pub const COMPUTER_MARK: Mark = Mark::O;

const MAX_DEPTH: usize = 16;
const MAX_SEARCH_TIME: f64 = 0.25; // seconds

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

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct InnerBoard {
    pub squares: [[Option<Mark>; 3]; 3],
    pub winner: Option<Mark>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OuterBoard {
    pub boards: [[InnerBoard; 3]; 3],
    pub overall_winner: Option<Mark>,
    pub active_square: Option<(u8, u8)>,
}

impl Default for OuterBoard {
    fn default() -> Self {
        Self {
            boards: Default::default(),
            overall_winner: Default::default(),
            active_square: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
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

    fn threats(&self, mark: Mark) -> usize {
        let mut threats = 0;

        for row in 0..3 {
            if self.squares[row]
                .iter()
                .filter(|&&cell| cell == Some(mark))
                .count()
                == 2
                && self.squares[row].iter().any(|&cell| cell.is_none())
            {
                threats += 1;
            }
        }

        for col in 0..3 {
            if (0..3)
                .map(|row| self.squares[row][col])
                .filter(|&cell| cell == Some(mark))
                .count()
                == 2
                && (0..3).any(|row| self.squares[row][col].is_none())
            {
                threats += 1;
            }
        }

        if (self.squares[0][0] == Some(mark)
            && self.squares[1][1] == Some(mark)
            && self.squares[2][2].is_none())
            || (self.squares[0][0].is_none()
                && self.squares[1][1] == Some(mark)
                && self.squares[2][2] == Some(mark))
            || (self.squares[0][0] == Some(mark)
                && self.squares[1][1].is_none()
                && self.squares[2][2] == Some(mark))
        {
            threats += 1;
        }

        if (self.squares[0][2] == Some(mark)
            && self.squares[1][1] == Some(mark)
            && self.squares[2][0].is_none())
            || (self.squares[0][2].is_none()
                && self.squares[1][1] == Some(mark)
                && self.squares[2][0] == Some(mark))
            || (self.squares[0][2] == Some(mark)
                && self.squares[1][1].is_none()
                && self.squares[2][0] == Some(mark))
        {
            threats += 1;
        }

        threats
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

        for row in 0..3 {
            if let Some(player) = self.boards[row][0].winner {
                if self.boards[row][1].winner == Some(player)
                    && self.boards[row][2].winner == Some(player)
                {
                    self.overall_winner = Some(player);
                    return;
                }
            }
        }

        for col in 0..3 {
            if let Some(player) = self.boards[0][col].winner {
                if self.boards[1][col].winner == Some(player)
                    && self.boards[2][col].winner == Some(player)
                {
                    self.overall_winner = Some(player);
                    return;
                }
            }
        }

        if let Some(player) = self.boards[1][1].winner {
            if (self.boards[0][0].winner == Some(player)
                && self.boards[2][2].winner == Some(player))
                || (self.boards[0][2].winner == Some(player)
                    && self.boards[2][0].winner == Some(player))
            {
                self.overall_winner = Some(player);
                return;
            }
        }
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

    pub fn computer_move(&self) -> Option<Move> {
        struct Searcher {
            start_time: std::time::Instant,
        }

        impl Searcher {
            fn search(
                &self,
                node: &OuterBoard,
                depth: usize,
                maximizing: bool,

                mut alpha: i32,
                mut beta: i32,
            ) -> i32 {
                if depth == 0
                    || node.overall_winner.is_some()
                    || std::time::Instant::now()
                        .saturating_duration_since(self.start_time)
                        .as_secs_f64()
                        > MAX_SEARCH_TIME
                {
                    return node.heuristic_score();
                }

                if maximizing {
                    let mut best_eval = i32::MIN;

                    for r#move in node.possible_moves(COMPUTER_MARK) {
                        let Some(child) = node.make_move(r#move) else { continue };
                        let eval = self.search(&child, depth - 1, !maximizing, alpha, beta);
                        best_eval = best_eval.max(eval);
                        alpha = alpha.max(eval);
                        if beta <= alpha {
                            break; // Beta cut-off
                        }
                    }

                    best_eval
                } else {
                    let mut best_eval = i32::MAX;
                    for r#move in node.possible_moves(HUMAN_MARK) {
                        let Some(child) = node.make_move(r#move) else { continue };
                        let eval = self.search(&child, depth - 1, !maximizing, alpha, beta);
                        best_eval = best_eval.min(eval);
                        beta = beta.min(eval);
                        if beta <= alpha {
                            break; // Alpha cut-off
                        }
                    }
                    best_eval
                }
            }
        }

        let searcher = Searcher {
            start_time: std::time::Instant::now(),
        };

        self.possible_moves(COMPUTER_MARK)
            .into_par_iter()
            .max_by_key(|&r#move| {
                let value = self.make_move(r#move)
                    .map_or(i32::MIN, |child| searcher.search(&child, MAX_DEPTH - 1, false, i32::MIN, i32::MAX));
                debug!("move" = ?r#move, "value" = value, "computer_move_opportunity");
                value
            })
    }

    fn heuristic_score(&self) -> i32 {
        // Immediate win/loss
        if let Some(winner) = self.overall_winner {
            return if winner == COMPUTER_MARK { i32::MAX } else { i32::MIN };
        }

        let mut score = 0;

        for row in 0..3 {
            for col in 0..3 {
                let inner_board = &self.boards[row][col];
                if let Some(winner) = inner_board.winner {
                    // Small board win/loss
                    if winner == COMPUTER_MARK {
                        score += 1000;
                    } else {
                        score -= 1000;
                    }
                } else {
                    // Threats
                    score += 100 * inner_board.threats(COMPUTER_MARK) as i32;
                    score -= 100 * inner_board.threats(HUMAN_MARK) as i32;

                    // Center control
                    match inner_board.squares[1][1] {
                        Some(COMPUTER_MARK) => score += 10,
                        Some(HUMAN_MARK) => score -= 10,
                        None => {}
                    }

                    // Edge control
                    for &(r, c) in &[(0, 1), (1, 0), (1, 2), (2, 1)] {
                        match inner_board.squares[r][c] {
                            Some(COMPUTER_MARK) => score += 5,
                            Some(HUMAN_MARK) => score -= 5,
                            None => {}
                        }
                    }

                    // Corner control
                    for &(r, c) in &[(0, 0), (0, 2), (2, 0), (2, 2)] {
                        match inner_board.squares[r][c] {
                            Some(COMPUTER_MARK) => score += 2,
                            Some(HUMAN_MARK) => score -= 2,
                            None => {}
                        }
                    }

                }
            }
        }

        score
    }
}

use std::sync::OnceLock;

use dashmap::DashMap;
use rayon::prelude::*;
use tracing::debug;

use super::{InnerBoard, Mark, Move, OuterBoard};

pub struct Searcher {
    pub start_time: std::time::Instant,
    pub player: Mark,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct TTableKey {
    board: OuterBoard,
    maximizing: bool,
    player: Mark,
}

#[allow(dead_code)]
struct TTableValue {
    value: i32,
    depth: usize,
}

static TRANSPOSITION_TABLE: OnceLock<DashMap<TTableKey, TTableValue>> = OnceLock::new();

const MAX_DEPTH: usize = 16;
const MAX_SEARCH_TIME: f64 = 0.25; // seconds

impl Searcher {
    pub fn search(board: &OuterBoard, player: Mark) -> Option<(Move, i32)> {
        TRANSPOSITION_TABLE.get_or_init(|| DashMap::new());

        let searcher = Self {
            start_time: std::time::Instant::now(),
            player,
        };

        board
            .possible_moves(player)
            .into_par_iter()
            .map(|r#move| {
                let value = board.make_move(r#move).map_or(i32::MIN, |child| {
                    searcher.branch(&child, MAX_DEPTH - 1, false, i32::MIN, i32::MAX)
                });
                debug!("move" = ?r#move, "value" = value, "computer_move_opportunity");
                (r#move, value)
            })
            .max_by_key(|&(_, value)| value)
    }

    fn branch(
        &self,
        node: &OuterBoard,
        depth: usize,
        maximizing: bool,

        mut alpha: i32,
        mut beta: i32,
    ) -> i32 {
        // let table = TRANSPOSITION_TABLE.get().unwrap();
        // let key = TableKey {
        //     board: *node,
        //     depth,
        //     maximizing,
        // };
        // if let Some(&cached) = table.get(&key).map(|entry| *entry.value()) && cached.depth >= depth {
        //     return cached;
        // }

        if depth == 0
            || node.overall_winner.is_some()
            || std::time::Instant::now()
                .saturating_duration_since(self.start_time)
                .as_secs_f64()
                > MAX_SEARCH_TIME
        {
            return Self::heuristic(
                node,
                self.player,
                if maximizing {
                    self.player
                } else {
                    !self.player
                },
            );
        }

        if maximizing {
            let mut best_eval = i32::MIN;

            for r#move in node.possible_moves(self.player) {
                let Some(child) = node.make_move(r#move) else {
                    continue;
                };
                let eval = self.branch(&child, depth - 1, !maximizing, alpha, beta);
                best_eval = best_eval.max(eval);
                alpha = alpha.max(eval);
                if beta <= alpha {
                    break; // Beta cut-off
                }
            }

            best_eval
        } else {
            let mut best_eval = i32::MAX;
            for r#move in node.possible_moves(!self.player) {
                let Some(child) = node.make_move(r#move) else {
                    continue;
                };
                let eval = self.branch(&child, depth - 1, !maximizing, alpha, beta);
                best_eval = best_eval.min(eval);
                beta = beta.min(eval);
                if beta <= alpha {
                    break; // Alpha cut-off
                }
            }
            best_eval
        }
    }

    fn heuristic(board: &OuterBoard, player: Mark, next_mark: Mark) -> i32 {
        let meta_board = board.meta_board();

        // Immediate win/loss
        if let Some(winner) = meta_board.winner {
            return if winner == player { i32::MAX } else { i32::MIN };
        }

        let mut score = 0;

        let score_inner = |score: &mut i32, inner_board: &InnerBoard, score_threats: bool| {
            if let Some(winner) = inner_board.winner {
                // Small board win/loss
                if winner == player {
                    *score += 1000;
                } else {
                    *score -= 1000;
                }
            } else {
                // Threats
                if score_threats {
                    *score += 100 * Self::threats(inner_board.squares, player) as i32;
                    *score -= 100 * Self::threats(inner_board.squares, !player) as i32;
                }

                // Center control
                if inner_board.squares[1][1] == Some(player) {
                    *score += 10;
                } else if inner_board.squares[1][1] == Some(!player) {
                    *score -= 10;
                }

                // Edge control
                for &(r, c) in &[(0, 1), (1, 0), (1, 2), (2, 1)] {
                    if inner_board.squares[r][c] == Some(player) {
                        *score += 5;
                    } else if inner_board.squares[r][c] == Some(!player) {
                        *score -= 5;
                    }
                }

                // Corner control
                for &(r, c) in &[(0, 0), (0, 2), (2, 0), (2, 2)] {
                    if inner_board.squares[r][c] == Some(player) {
                        *score += 2;
                    } else if inner_board.squares[r][c] == Some(!player) {
                        *score -= 2;
                    }
                }
            }
        };

        score_inner(&mut score, &meta_board, false);

        let meta_board_with_draws = board.meta_board_with_draws();
        score += 100 * Self::threats(meta_board_with_draws, Ok(player)) as i32;
        score -= 100 * Self::threats(meta_board_with_draws, Ok(!player)) as i32;
        score *= 5; // Meta board is more important

        for row in 0..3 {
            for col in 0..3 {
                let inner_board = &board.boards[row][col];
                score_inner(&mut score, inner_board, true);
            }
        }

        if board.active_square.is_none() {
            if next_mark == player {
                score += 200; // Favorable position when we can choose any board
            } else {
                score -= 200; // Unfavorable position when opponent can choose any board
            }
        }

        score
    }

    fn threats<T: Eq + Copy>(squares: [[Option<T>; 3]; 3], mark: T) -> usize {
        let mut threats = 0;

        for row in 0..3 {
            if squares[row]
                .iter()
                .filter(|&&cell| cell == Some(mark))
                .count()
                == 2
                && squares[row].iter().any(|&cell| cell.is_none())
            {
                threats += 1;
            }
        }

        for col in 0..3 {
            if (0..3)
                .map(|row| squares[row][col])
                .filter(|&cell| cell == Some(mark))
                .count()
                == 2
                && (0..3).any(|row| squares[row][col].is_none())
            {
                threats += 1;
            }
        }

        if (squares[0][0] == Some(mark) && squares[1][1] == Some(mark) && squares[2][2].is_none())
            || (squares[0][0].is_none()
                && squares[1][1] == Some(mark)
                && squares[2][2] == Some(mark))
            || (squares[0][0] == Some(mark)
                && squares[1][1].is_none()
                && squares[2][2] == Some(mark))
        {
            threats += 1;
        }

        if (squares[0][2] == Some(mark) && squares[1][1] == Some(mark) && squares[2][0].is_none())
            || (squares[0][2].is_none()
                && squares[1][1] == Some(mark)
                && squares[2][0] == Some(mark))
            || (squares[0][2] == Some(mark)
                && squares[1][1].is_none()
                && squares[2][0] == Some(mark))
        {
            threats += 1;
        }

        threats
    }
}

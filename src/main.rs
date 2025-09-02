use color_eyre::eyre::{Result, eyre};
use eframe::egui::{self, Rect};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use tracing::{error, info};

mod game;

#[derive(Clone, Copy, bincode::Encode, bincode::Decode)]
struct GameState {
    board: game::OuterBoard,
    last_player_move: Option<game::Move>,
    last_computer_move: Option<game::Move>,
    eval: i32,
}

impl GameState {
    fn root(board: game::OuterBoard) -> Self {
        Self {
            board,
            last_player_move: None,
            last_computer_move: None,
            eval: 0,
        }
    }
}

struct App {
    random_fill_percentage: f64,

    req_tx: SyncSender<(game::Mark, game::OuterBoard)>,
    resp_rx: Receiver<Option<(game::Move, i32)>>,
    thinking: bool,

    states: Vec<GameState>,
}

impl Default for App {
    fn default() -> Self {
        let (req_tx, req_rx) = sync_channel::<(game::Mark, game::OuterBoard)>(1);
        let (resp_tx, resp_rx) = sync_channel::<Option<(game::Move, i32)>>(1);

        std::thread::spawn(move || {
            for (player, state) in req_rx {
                let result = resp_tx.send(state.best_move(player));
                if result.is_err() {
                    break;
                }
            }
        });

        Self {
            random_fill_percentage: 0.5,
            req_tx,
            resp_rx,
            thinking: false,
            states: vec![],
        }
    }
}

#[derive(Clone, Copy)]
struct GridHelper {
    #[allow(dead_code)]
    screen: egui::Rect,
    rect: egui::Rect,
}

impl GridHelper {
    fn new(screen: egui::Rect) -> Self {
        let square_size = screen.width().min(screen.height());
        let rect =
            egui::Rect::from_center_size(screen.center(), egui::vec2(square_size, square_size));
        Self { screen, rect }
    }

    fn position(&self, row: u8, col: u8) -> egui::Pos2 {
        let x = self.rect.left() + self.rect.width() / 3.0 * col as f32 + self.rect.width() / 6.0;
        let y = self.rect.top() + self.rect.height() / 3.0 * row as f32 + self.rect.height() / 6.0;
        egui::pos2(x, y)
    }

    fn square_size(&self) -> f32 {
        self.rect.width() / 3.0
    }

    fn subgrid(&self, row: u8, col: u8) -> Self {
        let cell_w = self.rect.width() / 3.0;
        let cell_h = self.rect.height() / 3.0;
        let left = self.rect.left() + col as f32 * cell_w;
        let top = self.rect.top() + row as f32 * cell_h;
        let new_screen = Rect::from_min_size(egui::pos2(left, top), egui::vec2(cell_w, cell_h));
        Self::new(new_screen.shrink(cell_w.min(cell_h) * 0.05))
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_thread_ids(true))
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_target("supertris", tracing::Level::TRACE)
                .with_default(tracing::Level::INFO),
        )
        .try_init()?;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_icon(eframe::icon_data::from_png_bytes(
            include_bytes!("../assets/icon.png"),
        )?),
        ..Default::default()
    };
    eframe::run_native(
        "Supertris",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
    .map_err(|e| eyre!("{e:?}"))?;
    Ok(())
}

impl App {
    fn board(&self) -> game::OuterBoard {
        self.states
            .last()
            .map_or_else(|| game::OuterBoard::default(), |s| s.board)
    }

    fn eval(&self) -> i32 {
        self.states.last().map_or(0, |s| s.eval)
    }

    fn last_player_move(&self) -> Option<game::Move> {
        self.states.last().and_then(|s| s.last_player_move)
    }

    fn last_computer_move(&self) -> Option<game::Move> {
        self.states.last().and_then(|s| s.last_computer_move)
    }

    fn overall_winner(&self) -> Option<game::Mark> {
        self.states.last().map_or(None, |s| s.board.overall_winner)
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading("Supertris");
            ui.horizontal(|ui| {
                ui.label("Basta col solito tris, prova Supertris!");
            });
            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Reset").clicked() {
                    *self = App::default();
                }

                let random_btn = ui.button("Partita a caso");
                if random_btn.clicked() {
                    self.states.clear();
                    self.states.push(GameState::root(game::OuterBoard::random(
                        self.random_fill_percentage,
                    )));
                }

                if ui.button("Gioca per me").clicked() {
                    if !self.thinking {
                        self.req_tx.send((game::HUMAN_MARK, self.board())).unwrap();
                        self.thinking = true;
                    }
                }

                if ui.button("Annulla mossa").clicked() {
                    assert!(!self.thinking);
                    self.states.pop();
                }
            });
            ui.add(
                egui::Slider::new(&mut self.random_fill_percentage, 0.0..=1.0)
                    .text("Percentuale di caselle riempite"),
            );

            ui.separator();

            ui.vertical_centered(|ui| {
                let mut left_font_size = 1.0f32;
                let mut right_font_size = 256.0f32;

                // binary search
                loop {
                    if (right_font_size - left_font_size).abs() < 1.0 {
                        break;
                    }

                    let font_size = (left_font_size + right_font_size) / 2.0;

                    let header_width = ui
                        .painter()
                        .layout_no_wrap(
                            "Valutazione:".to_string(),
                            egui::FontId::proportional(font_size),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    let num_width = ui
                        .painter()
                        .layout_no_wrap(
                            format!("{}", self.eval()),
                            egui::FontId::proportional(font_size),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    let w = header_width.max(num_width);

                    if w > ui.available_width() * 0.9 {
                        right_font_size = font_size;
                    } else if w < ui.available_width() * 0.9 {
                        left_font_size = font_size;
                    } else {
                        break;
                    }
                }
                let font_size = (left_font_size + right_font_size) / 2.0;
                ui.label(
                    egui::RichText::new("Valutazione:").font(egui::FontId::proportional(font_size)),
                );
                ui.label(
                    egui::RichText::new(format!("{}", self.eval()))
                        .font(egui::FontId::proportional(font_size))
                        .color(if self.eval() < 0 {
                            egui::Color32::RED
                        } else if self.eval() > 0 {
                            egui::Color32::BLUE
                        } else {
                            egui::Color32::YELLOW
                        }),
                );
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Salva").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("Salva partita")
                        .set_file_name("supertris_save.bin")
                        .save_file()
                {
                    bincode::encode_into_std_write(
                        &self.states,
                        &mut std::fs::File::create(&path).unwrap(),
                        bincode::config::standard(),
                    )
                    .unwrap();
                    info!(path = %path.display(), "game_saved");
                }
                if ui.button("Carica").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("Carica partita")
                        .set_file_name("supertris_save.bin")
                        .add_filter("Binary save file", &["bin"])
                        .pick_file()
                {
                    let rfp = self.random_fill_percentage;
                    *self = App::default();
                    self.random_fill_percentage = rfp;
                    self.states = bincode::decode_from_std_read(
                        &mut std::fs::File::open(&path).unwrap(),
                        bincode::config::standard(),
                    )
                    .unwrap_or_else(|e| {
                        error!(error = ?e, "save_load_error");
                        vec![]
                    });
                    info!(path = %path.display(), "game_loaded");
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            draw_game(ui, self);
        });
    }
}

fn draw_game(ui: &mut egui::Ui, app: &mut App) {
    let gh = GridHelper::new(ui.max_rect());

    if app.thinking {
        if let Ok(computer_move) = app.resp_rx.try_recv() {
            if let Some((r#move, eval)) = computer_move {
                let _span = tracing::debug_span!("computer_move", "move" = ?r#move, eval).entered();
                if let Some(new_board) = app.board().make_move(r#move) {
                    info!("computer_move_done");

                    let is_fake_human = r#move.player != game::COMPUTER_MARK;

                    if is_fake_human {
                        let mut new_state = app
                            .states
                            .last()
                            .copied()
                            .unwrap_or_else(|| GameState::root(game::OuterBoard::default()));

                        new_state.last_player_move = Some(r#move);
                        new_state.board = new_board;
                        new_state.eval = eval;

                        app.states.push(new_state);
                        app.req_tx.send((game::COMPUTER_MARK, app.board())).unwrap();
                        app.thinking = true;
                    } else {
                        let state = app.states.last_mut().unwrap();
                        state.last_computer_move = Some(r#move);
                        state.board = new_board;
                        state.eval = eval;
                        app.thinking = false;
                    }
                } else {
                    error!("computer_move_invalid");
                    app.thinking = false;
                }
            } else {
                error!("no_computer_move");
                app.thinking = false;
            }
        } else {
            egui::Modal::new("thinking_modal".into()).show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.label("Thinking...");
                    ui.spinner();
                });
            });
        }
    }

    draw_grid_lines(
        ui,
        gh,
        app.overall_winner().is_none() && app.board().active_square.is_none(),
    );

    let mut player_move = None;

    for row in 0..3 {
        for col in 0..3 {
            let inner_board = &mut app.board().boards[row as usize][col as usize];
            let sub_gh = gh.subgrid(row, col);

            draw_grid_lines(ui, sub_gh, app.board().active_square == Some((row, col)));

            for inner_row in 0..3 {
                for inner_col in 0..3 {
                    if draw_grid_item(
                        ui,
                        sub_gh,
                        inner_row,
                        inner_col,
                        inner_board.squares[inner_row as usize][inner_col as usize],
                        app.overall_winner().is_none()
                            && (app.last_computer_move().is_some_and(|m| {
                                m.outer == (row, col) && m.inner == (inner_row, inner_col)
                            }) || app.last_player_move().is_some_and(|m| {
                                m.outer == (row, col) && m.inner == (inner_row, inner_col)
                            })),
                    )
                    .clicked()
                    {
                        player_move = Some(game::Move {
                            outer: (row, col),
                            inner: (inner_row, inner_col),
                            player: game::HUMAN_MARK,
                        });
                    }
                }
            }

            if let Some(winner) = inner_board.winner {
                draw_opacizing_square(ui, sub_gh);
                draw_grid_item(
                    ui,
                    gh,
                    row,
                    col,
                    Some(winner),
                    app.board().overall_winner.is_none()
                        && (app
                            .last_computer_move()
                            .is_some_and(|m| m.outer == (row, col))
                            || app
                                .last_player_move()
                                .is_some_and(|m| m.outer == (row, col))),
                );
            }
        }
    }

    if !app.thinking
        && let Some(player_move) = player_move
        && let Some(new_board) = app.board().make_move(player_move)
    {
        info!("move" = ?player_move, "player_move_done");
        let mut new_state = app
            .states
            .last()
            .copied()
            .unwrap_or_else(|| GameState::root(game::OuterBoard::default()));
        new_state.board = new_board;
        new_state.last_player_move = Some(player_move);
        new_state.eval = 0;
        app.states.push(new_state);

        app.req_tx.send((game::COMPUTER_MARK, app.board())).unwrap();
        app.thinking = true;
    }

    if app.overall_winner().is_some() {
        let t = (ui.ctx().input(|i| i.time).sin() + 1.0) / 2.0;
        let scale = (0.85 - 0.5) * t as f32 + 0.5;

        draw_opacizing_square(ui, gh);
        draw_filled_square(
            ui.painter(),
            gh.rect.center().x,
            gh.rect.center().y,
            gh.rect.width() / 2.0 * scale,
            app.overall_winner().unwrap(),
            false,
        );

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn draw_opacizing_square(ui: &mut egui::Ui, gh: GridHelper) {
    let painter = ui.painter();
    painter.rect(
        gh.rect,
        3.0,
        egui::Color32::from_rgba_unmultiplied(0xe1, 0xe1, 0xe1, 150),
        egui::Stroke::NONE,
        egui::StrokeKind::Inside,
    );
}

fn draw_grid_item(
    ui: &mut egui::Ui,
    gh: GridHelper,
    row: u8,
    col: u8,
    square: Option<game::Mark>,
    highlight: bool,
) -> egui::Response {
    let painter = ui.painter();

    let egui::Pos2 { x, y } = gh.position(row, col);
    let radius = gh.square_size() / 2.0 * 0.85;

    if let Some(square) = square {
        draw_filled_square(painter, x, y, radius, square, highlight);
    }

    ui.interact(
        egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(radius * 2.0, radius * 2.0)),
        ui.id().with((x as u32, y as u32)),
        egui::Sense::click(),
    )
}

fn draw_filled_square(
    painter: &egui::Painter,
    x: f32,
    y: f32,
    radius: f32,
    square: game::Mark,
    highlight: bool,
) {
    let color = match (square, highlight) {
        (game::Mark::X, false) => egui::Color32::RED,
        (game::Mark::X, true) => egui::Color32::from_rgb(255, 105, 180), // light red
        (game::Mark::O, false) => egui::Color32::BLUE,
        (game::Mark::O, true) => egui::Color32::from_rgb(135, 206, 250), // light blue
    };
    let stroke_width = if highlight { 4.0 } else { 2.0 };

    match square {
        game::Mark::X => {
            painter.line_segment(
                [
                    egui::pos2(x - radius, y - radius),
                    egui::pos2(x + radius, y + radius),
                ],
                egui::Stroke::new(stroke_width, color),
            );
            painter.line_segment(
                [
                    egui::pos2(x + radius, y - radius),
                    egui::pos2(x - radius, y + radius),
                ],
                egui::Stroke::new(stroke_width, color),
            );
        }
        game::Mark::O => {
            painter.circle(
                egui::Pos2 { x, y },
                radius,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(stroke_width, color),
            );
        }
    }
}

fn draw_grid_lines(ui: &mut egui::Ui, gh: GridHelper, highlight: bool) {
    let painter = ui.painter();

    // grid config
    let rows = 3;
    let cols = 3;
    let stroke = egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY);

    let cell_w = gh.rect.width() / cols as f32;
    let cell_h = gh.rect.height() / rows as f32;

    if highlight {
        painter.rect(
            gh.rect.shrink(-4.0),
            3.0,
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(4.0, egui::Color32::GREEN),
            egui::StrokeKind::Outside,
        );
    }

    // vertical lines
    for i in 1..cols {
        let x = gh.rect.left() + i as f32 * cell_w;
        painter.line_segment(
            [
                egui::pos2(x, gh.rect.top()),
                egui::pos2(x, gh.rect.bottom()),
            ],
            stroke,
        );
    }

    // horizontal lines
    for j in 1..rows {
        let y = gh.rect.top() + j as f32 * cell_h;
        painter.line_segment(
            [
                egui::pos2(gh.rect.left(), y),
                egui::pos2(gh.rect.right(), y),
            ],
            stroke,
        );
    }
}

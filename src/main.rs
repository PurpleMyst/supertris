use color_eyre::eyre::{Result, eyre};
use eframe::egui::{self, Rect};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use tracing::{error, info};

mod game;

struct App {
    board: game::OuterBoard,

    random_fill_percentage: f64,

    req_tx: SyncSender<game::OuterBoard>,
    resp_rx: Receiver<Option<game::Move>>,
    thinking: bool,
}

impl Default for App {
    fn default() -> Self {
        let (req_tx, req_rx) = sync_channel::<game::OuterBoard>(1);
        let (resp_tx, resp_rx) = sync_channel::<Option<game::Move>>(1);

        std::thread::spawn(move || {
            for state in req_rx {
                let result = resp_tx.send(state.computer_move());
                if result.is_err() {
                    break;
                }
            }
        });

        Self {
            board: Default::default(),
            random_fill_percentage: 0.5,
            req_tx,
            resp_rx,
            thinking: false,
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

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("side_panel").show(ctx, |ui| {
            ui.heading("Supertris");
            ui.horizontal(|ui| {
                ui.label("Basta col solito tris, prova Supertris!");
            });
            ui.separator();
            if ui.button("Ricomincia").clicked() {
                *self = App::default();
            }

            let random_btn = ui.button("A caso");
            ui.add(
                egui::Slider::new(&mut self.random_fill_percentage, 0.0..=1.0)
                    .text("Fill Percentage"),
            );
            if random_btn.clicked() {
                self.board = game::OuterBoard::random(self.random_fill_percentage);
            }
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
            if let Some(r#move) = computer_move {
                let _span = tracing::debug_span!("computer_move", "move" = ?r#move).entered();
                if let Some(new_board) = app.board.make_move(r#move) {
                    info!("computer_move_done");
                    app.board = new_board;
                } else {
                    error!("computer_move_invalid");
                }
            } else {
                error!("no_computer_move");
            }
            app.thinking = false;
        } else {
            egui::Modal::new("thinking_modal".into()).show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.label("Thinking...");
                    ui.spinner();
                });
            });
        }
    }

    if app.board.overall_winner.is_some() {
        draw_filled_square(
            ui.painter(),
            gh.rect.center().x,
            gh.rect.center().y,
            gh.rect.width() / 2.0 * 0.85,
            app.board.overall_winner.unwrap(),
        );
        return;
    }

    draw_grid_lines(ui, gh, app.board.active_square.is_none());

    let mut player_move = None;

    for row in 0..3 {
        for col in 0..3 {
            let inner_board = &mut app.board.boards[row as usize][col as usize];

            if let Some(winner) = inner_board.winner {
                draw_grid_item(ui, gh, row, col, Some(winner));
                continue;
            }

            let sub_gh = gh.subgrid(row, col);

            // draw_grid_lines(ui, sub_gh, (row, col) == app.board.active_square);
            draw_grid_lines(ui, sub_gh, app.board.active_square == Some((row, col)));

            for inner_row in 0..3 {
                for inner_col in 0..3 {
                    if draw_grid_item(
                        ui,
                        sub_gh,
                        inner_row,
                        inner_col,
                        inner_board.squares[inner_row as usize][inner_col as usize],
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
        }
    }

    if !app.thinking
        && let Some(player_move) = player_move
        && let Some(new_board) = app.board.make_move(player_move)
    {
        info!("move" = ?player_move, "player_move_done");
        app.board = new_board;

        app.req_tx.send(app.board).unwrap();
        app.thinking = true;
    }
}

fn draw_grid_item(
    ui: &mut egui::Ui,
    gh: GridHelper,
    row: u8,
    col: u8,
    square: Option<game::Mark>,
) -> egui::Response {
    let painter = ui.painter();

    let egui::Pos2 { x, y } = gh.position(row, col);
    let radius = gh.square_size() / 2.0 * 0.85;

    if let Some(square) = square {
        draw_filled_square(painter, x, y, radius, square);
    }

    ui.interact(
        egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(radius * 2.0, radius * 2.0)),
        ui.id().with((x as u32, y as u32)),
        egui::Sense::click(),
    )
}

fn draw_filled_square(painter: &egui::Painter, x: f32, y: f32, radius: f32, square: game::Mark) {
    let color = match square {
        game::Mark::X => egui::Color32::RED,
        game::Mark::O => egui::Color32::BLUE,
    };

    match square {
        game::Mark::X => {
            painter.line_segment(
                [
                    egui::pos2(x - radius, y - radius),
                    egui::pos2(x + radius, y + radius),
                ],
                egui::Stroke::new(2.0, color),
            );
            painter.line_segment(
                [
                    egui::pos2(x + radius, y - radius),
                    egui::pos2(x - radius, y + radius),
                ],
                egui::Stroke::new(2.0, color),
            );
        }
        game::Mark::O => {
            painter.circle(
                egui::Pos2 { x, y },
                radius,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new(2.0, color),
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

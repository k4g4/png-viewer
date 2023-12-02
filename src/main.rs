#![windows_subsystem = "windows"]
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use iced::{
    alignment, executor, mouse, theme,
    widget::{
        self,
        canvas::{self, Cache, Frame, Geometry, Program},
        column, row, Canvas,
    },
    window, Application, Command, Element, Length, Rectangle, Renderer, Settings, Theme, Vector,
};
use rand::thread_rng;
use tracing::{debug, info};

const PHOTO_ICON: &[u8] = include_bytes!("../assets/photo.ico");
const SIZE: (u32, u32) = (700, 700);
const MIN_SIZE: (u32, u32) = (200, 400);
const EMOJIS: &str = "🌄🌅🌇🌠🌉🏕️";

fn main() -> iced::Result {
    tracing_subscriber::fmt::fmt()
        .with_env_filter("png_viewer")
        .init();

    App::run(Settings {
        window: window::Settings {
            size: SIZE,
            position: window::Position::Centered,
            min_size: Some(MIN_SIZE),
            icon: Some(window::icon::from_file_data(PHOTO_ICON, None).unwrap()),
            ..window::Settings::default()
        },
        ..Settings::default()
    })
}

#[derive(Default)]
struct App {
    viewer: Viewer,
}

#[derive(Debug, Clone)]
enum Message {
    ImageLoaded(Arc<native_dialog::Result<Option<PathBuf>>>),
    LoadImage,
}

impl Application for App {
    type Executor = executor::Default;

    type Message = Message;

    type Theme = Theme;

    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        "PNG Viewer".into()
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::ImageLoaded(result) => {
                match result.as_ref() {
                    Ok(Some(image_path)) => {
                        info!("Loading: {}", image_path.display());
                        self.viewer.load_png(image_path);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        debug!("{err:?}");
                        let _ = native_dialog::MessageDialog::new()
                            .set_title("Error loading image")
                            .set_text(&format!("{err:?}"))
                            .show_alert();
                    }
                }

                Command::none()
            }
            Message::LoadImage => Command::perform(
                async {
                    Arc::new(
                        native_dialog::FileDialog::new()
                            .set_title("Load Image")
                            .show_open_single_file(),
                    )
                },
                Message::ImageLoaded,
            ),
        }
    }

    fn view(&self) -> Element<'_, Self::Message, Renderer<Self::Theme>> {
        struct ButtonTheme;

        impl widget::button::StyleSheet for ButtonTheme {
            type Style = Theme;

            fn active(&self, style: &Self::Style) -> widget::button::Appearance {
                widget::button::Appearance {
                    border_radius: 15.0.into(),
                    ..style.active(&theme::Button::Primary)
                }
            }
        }

        let bottom_bar = row![
            widget::horizontal_space(Length::Fill),
            widget::button("Load Image")
                .on_press(Message::LoadImage)
                .style(theme::Button::custom(ButtonTheme))
                .padding(10),
            widget::horizontal_space(Length::Fill)
        ]
        .padding(20);

        column![
            Canvas::new(&self.viewer)
                .height(Length::Fill)
                .width(Length::Fill),
            widget::container("")
                .style(|theme: &Theme| widget::container::Appearance {
                    border_width: 2.0,
                    border_color: theme.palette().primary,
                    ..Default::default()
                })
                .width(Length::Fill)
                .max_height(2),
            bottom_bar,
        ]
        .into()
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}

enum Viewer {
    Png { png_cache: Cache },
    None(char),
}

impl Viewer {
    fn load_png(&mut self, _png_path: impl AsRef<Path>) {
        *self = Self::Png {
            png_cache: Cache::new(),
        }
    }
}

impl Default for Viewer {
    fn default() -> Self {
        use rand::seq::IteratorRandom;

        let mut rng = thread_rng();
        Self::None(EMOJIS.chars().choose(&mut rng).unwrap())
    }
}

impl Program<Message> for Viewer {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer<Theme>,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        match self {
            Self::Png { png_cache } => vec![png_cache.draw(renderer, bounds.size(), |_frame| {})],
            Self::None(emoji) => {
                let mut frame = Frame::new(renderer, bounds.size());
                frame.translate(Vector::new(bounds.width * 0.5, bounds.height * 0.2));
                frame.fill_text(canvas::Text {
                    content: emoji.to_string(),
                    shaping: widget::text::Shaping::Advanced,
                    size: 100.0 + bounds.height * 0.4,
                    horizontal_alignment: alignment::Horizontal::Center,
                    ..Default::default()
                });
                vec![frame.into_geometry()]
            }
        }
    }

    fn update(
        &self,
        _state: &mut Self::State,
        _event: canvas::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        (canvas::event::Status::Ignored, None)
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

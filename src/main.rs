// uncomment for release: #![windows_subsystem = "windows"]

use png_viewer::render;

use iced::{
    alignment, executor, mouse, theme,
    widget::{
        self,
        canvas::{self, Cache, Frame, Geometry, Program},
        column, row, Canvas,
    },
    window, Application, Command, Element, Length, Rectangle, Renderer, Settings, Theme, Vector,
};
use tokio::sync::oneshot;

const SIZE: (u32, u32) = (700, 700);
const MIN_SIZE: (u32, u32) = (200, 400);
const PHOTO_ICON: &[u8] = include_bytes!("../assets/photo.ico");
const EMOJIS: &[char] = &['🌄', '🌅', '🌇', '🌠', '🌉', '🏡', '🌺', '⛵', '🪐', '🌞'];

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
    Load,
    Loaded,
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
            Message::Load => self.viewer.load(),
            Message::Loaded => self.viewer.loaded(),
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

        let open_button = widget::button("Open PNG")
            .style(theme::Button::custom(ButtonTheme))
            .padding(10)
            .on_press(Message::Load);

        let bottom_bar = row![
            widget::horizontal_space(Length::Fill),
            open_button,
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
    Viewing {
        data: Vec<u8>,
        cache: Cache,
    },
    Loading {
        load_recv: oneshot::Receiver<std::io::Result<Vec<u8>>>,
    },
    Empty {
        emoji: char,
    },
}

impl Viewer {
    fn load(&mut self) -> Command<Message> {
        match native_dialog::FileDialog::new()
            .set_title("Open PNG")
            .show_open_single_file()
        {
            Ok(Some(path)) => {
                tracing::debug!("Loading: {}", path.display());
                let (load_send, load_recv) = oneshot::channel();
                *self = Self::Loading { load_recv };
                Command::perform(tokio::fs::read(path), |result| {
                    let _ = load_send.send(result);
                    Message::Loaded
                })
            }

            Ok(None) => {
                tracing::debug!("No file selected");
                Command::none()
            }

            Err(error) => {
                tracing::error!("from native_dialog::FileDialog: {error}");
                Command::none()
            }
        }
    }

    fn loaded(&mut self) -> Command<Message> {
        match self {
            Self::Loading { load_recv } => match load_recv.try_recv() {
                Ok(Ok(data)) => {
                    *self = Self::Viewing {
                        data,
                        cache: Cache::new(),
                    };
                }
                Ok(Err(error)) => {
                    tracing::error!("from tokio::fs::read: {error}");
                }
                Err(error) => {
                    tracing::error!("from load_recv.try_recv: {error}");
                }
            },
            _ => {
                tracing::error!("Viewer::loaded called on non-Loading variant");
            }
        }
        Command::none()
    }
}

impl Default for Viewer {
    fn default() -> Self {
        use rand::seq::SliceRandom;

        Self::Empty {
            emoji: *EMOJIS.choose(&mut rand::thread_rng()).unwrap(),
        }
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
            Self::Viewing { data, cache } => {
                vec![cache.draw(renderer, bounds.size(), |frame| {
                    if let Err(error) = render::render(frame, data) {
                        tracing::error!("from render::render: {error}");
                    }
                })]
            }

            Self::Loading { .. } => vec![],

            Self::Empty { emoji } => {
                let mut frame = Frame::new(renderer, bounds.size());
                frame.translate(Vector::new(bounds.width * 0.5, bounds.height * 0.25));
                frame.fill_text(canvas::Text {
                    content: emoji.to_string(),
                    shaping: widget::text::Shaping::Advanced,
                    size: 100.0 + bounds.height * 0.3,
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

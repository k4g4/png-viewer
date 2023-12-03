// uncomment for release: #![windows_subsystem = "windows"]

mod load;

use std::{io, sync::Arc};

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
use tokio::sync::oneshot;
use tracing::{debug, error};

const PHOTO_ICON: &[u8] = include_bytes!("../assets/photo.ico");
const SIZE: (u32, u32) = (700, 700);
const MIN_SIZE: (u32, u32) = (200, 400);
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
    Loaded(Arc<io::Result<()>>),
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
            Message::Loaded(result) => {
                self.viewer.loaded(result.as_ref());
                Command::none()
            }
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
            .padding(10);
        let open_button = if self.viewer.is_loading() {
            open_button
        } else {
            open_button.on_press(Message::Load)
        };

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
    Loading {
        cache_recv: oneshot::Receiver<Cache>,
    },
    Loaded {
        cache: Cache,
    },
    Unloaded {
        emoji: char,
    },
}

impl Viewer {
    fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    fn load(&mut self) -> Command<Message> {
        if self.is_loading() {
            error!("called Viewer::load while still loading");
            Command::none()
        } else {
            match native_dialog::FileDialog::new()
                .set_title("Open PNG")
                .show_open_single_file()
            {
                Ok(Some(path)) => {
                    debug!("Loading: {}", path.display());
                    let (cache_send, cache_recv) = oneshot::channel();
                    *self = Self::Loading { cache_recv };
                    Command::perform(load::load(path, cache_send), |result| {
                        Message::Loaded(Arc::new(result))
                    })
                }

                Ok(None) => {
                    debug!("No file selected");
                    Command::none()
                }

                Err(error) => {
                    error!("from native_dialog::FileDialog: {error:?}");
                    Command::none()
                }
            }
        }
    }

    fn loaded(&mut self, result: &io::Result<()>) {
        match self {
            Self::Loading { cache_recv } => {
                if let Err(error) = result {
                    error!("from load::load: {error:?}");
                } else {
                    match cache_recv.try_recv() {
                        Ok(cache) => {
                            *self = Self::Loaded { cache };
                        }
                        Err(error) => {
                            error!("from cache_recv.try_recv(): {error:?}");
                        }
                    }
                }
            }

            _ => {
                error!("called Viewer::loaded on a non-Loading variant");
            }
        }
    }
}

impl Default for Viewer {
    fn default() -> Self {
        use rand::seq::SliceRandom;

        Self::Unloaded {
            emoji: *EMOJIS.choose(&mut thread_rng()).unwrap(),
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
            Self::Loaded { cache } => vec![cache.draw(renderer, bounds.size(), |_| ())],

            Self::Loading { .. } => vec![],

            Self::Unloaded { emoji } => {
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

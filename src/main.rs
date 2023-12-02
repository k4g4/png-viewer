use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use iced::{
    executor, mouse,
    widget::{
        self,
        canvas::{self, Cache, Frame, Geometry, Program},
        column, row, Canvas,
    },
    window, Application, Command, Element, Length, Rectangle, Renderer, Settings, Theme,
};
use tracing::{info, level_filters::LevelFilter};

const PHOTO_ICON: &[u8] = include_bytes!("../assets/photo.png");

fn main() -> iced::Result {
    tracing_subscriber::fmt::fmt()
        .with_max_level(LevelFilter::INFO)
        .init();
    App::run(Settings {
        window: window::Settings {
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
        let bottom_bar = row![
            widget::horizontal_space(Length::Fill),
            widget::button("Load Image")
                .on_press(Message::LoadImage)
                .padding(5),
            widget::horizontal_space(Length::Fill)
        ]
        .padding(20);

        column![
            Canvas::new(&self.viewer).height(Length::Fill),
            widget::container("")
                .style(|theme: &Theme| widget::container::Appearance {
                    border_width: 2.0,
                    border_color: theme.palette().primary,
                    ..Default::default()
                })
                .width(Length::Fill)
                .max_height(1),
            bottom_bar,
        ]
        .into()
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}

#[derive(Default)]
enum Viewer {
    Png {
        png_cache: Cache,
    },
    #[default]
    None,
}

impl Viewer {
    fn load_png(&mut self, _png_path: impl AsRef<Path>) {
        *self = Self::Png {
            png_cache: Cache::new(),
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
            Self::Png { png_cache } => vec![png_cache.draw(renderer, bounds.size(), |_frame| {})],
            Self::None => {
                let frame = Frame::new(renderer, bounds.size());
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

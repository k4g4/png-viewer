use iced::widget::canvas;
use nom::{bytes::complete::tag, error::Error as NomError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("nom needed: {0:?}")]
    NomNeeded(nom::Needed),
    #[error("nom failed: {0:?}")]
    NomFailed(nom::error::ErrorKind),
}

impl From<nom::Err<NomError<&[u8]>>> for Error {
    fn from(nom_error: nom::Err<NomError<&[u8]>>) -> Self {
        use nom::Err::*;

        match nom_error {
            Incomplete(needed) => Self::NomNeeded(needed),
            Error(inner_error) | Failure(inner_error) => Self::NomFailed(inner_error.code),
        }
    }
}

pub fn render(frame: &mut canvas::Frame, data: &[u8]) -> Result<(), Error> {
    let (_, data) = tag(&[0x89])(data)?;
    let (_, data) = tag(b"PNG")(data)?;
    let (_, data) = tag(&[0x0D, 0x0A, 0x1A, 0x0A])(data)?;
    Ok(())
}

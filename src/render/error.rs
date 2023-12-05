use nom::HexDisplay;

type NomError = nom::error::Error<String>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("file parsing failed with error: {:?}; data:\n{}", .0.code, .0.input)]
    NomFailed(NomError),

    #[error("unknown critical chunk type found: {0}")]
    UnknownCriticalChunk(String),

    #[error("Invalid bit depth: {0}")]
    InvalidBitDepth(u8),

    #[error("Invalid color type: {0}")]
    InvalidColorType(u8),

    #[error("Invalid interlace method: {0}")]
    InvalidInterlace(u8),

    #[error("Critical chunk not found: {0}")]
    MissingCritical(&'static str),
}

impl nom::error::ParseError<&[u8]> for Error {
    fn from_error_kind(input: &[u8], kind: nom::error::ErrorKind) -> Self {
        Self::NomFailed(NomError {
            input: input[..input.len().min(64)].to_hex(8),
            code: kind,
        })
    }

    fn append(_input: &[u8], _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

impl From<nom::Err<Error>> for Error {
    fn from(nom_error: nom::Err<Error>) -> Self {
        use nom::Err::*;

        match nom_error {
            Incomplete(_) => unreachable!("Incomplete(..) never returned by complete parsers"),
            Error(inner_error) | Failure(inner_error) => inner_error,
        }
    }
}

impl nom::error::FromExternalError<&[u8], Self> for Error {
    fn from_external_error(_input: &[u8], _kind: nom::error::ErrorKind, e: Self) -> Self {
        e
    }
}

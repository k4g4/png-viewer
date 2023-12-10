use nom::HexDisplay;

type NomError = nom::error::Error<DbgString>;

#[derive(Debug, thiserror::Error, Default)]
pub enum Error {
    #[default]
    #[error("unknown error occurred")]
    Unknown,

    #[error("file parsing failed with error: {}; data:{:?}", .0.code.description(), .0.input)]
    NomFailed(NomError),

    #[error("unknown critical chunk type found: {0}")]
    UnknownCriticalChunk(String),

    #[error("invalid bit depth: {0}")]
    InvalidBitDepth(u8),

    #[error("invalid color type: {0}")]
    InvalidColorType(u8),

    #[error("invalid bit depth ({0}) and color type ({1}) combination")]
    InvalidBitColorCombo(u8, u8),

    #[error("invalid interlace method: {0}")]
    InvalidInterlace(u8),

    #[error("invalid palette size: {0}")]
    InvalidPaletteSize(usize),

    #[error("invalid filter type: {0}")]
    InvalidFilterType(u8),

    #[error("critical chunk not found: {0}")]
    MissingCritical(&'static str),

    #[error("invalid IEND chunk found")]
    InvalidIEnd,

    #[error("duplicate IHDR chunk found")]
    DuplicateIhdr,
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        error
            .into_inner()
            .and_then(|boxed| boxed.downcast::<Error>().ok())
            .map(|boxed| *boxed)
            .unwrap_or_default()
    }
}

pub struct DbgString(String);

impl std::fmt::Debug for DbgString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\n{}", self.0)
    }
}

impl nom::error::ParseError<&[u8]> for Error {
    fn from_error_kind(input: &[u8], kind: nom::error::ErrorKind) -> Self {
        Self::NomFailed(NomError {
            input: DbgString(input[..input.len().min(64)].to_hex(8)),
            code: kind,
        })
    }

    fn append(_input: &[u8], _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

impl nom::error::ParseError<(&[u8], usize)> for Error {
    fn from_error_kind(input: (&[u8], usize), kind: nom::error::ErrorKind) -> Self {
        Error::from_error_kind(input.0, kind)
    }

    fn append(input: (&[u8], usize), kind: nom::error::ErrorKind, other: Self) -> Self {
        Error::append(input.0, kind, other)
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

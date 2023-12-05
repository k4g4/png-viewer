mod chunks;
mod parse;

use iced::widget::canvas;

type NomError = nom::error::Error<Vec<u8>>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("file parsing failed with error: {:?}, data: {:#?}", .0.code, .0.input)]
    NomFailed(NomError),

    #[error("unknown critical chunk type found: {0}")]
    UnknownCriticalChunk(String),

    #[error("Invalid bit depth: {0}")]
    InvalidBitDepth(u8),

    #[error("Invalid color type: {0}")]
    InvalidColorType(u8),

    #[error("Invalid interlace method: {0}")]
    InvalidInterlace(u8),
}

impl nom::error::ParseError<&[u8]> for Error {
    fn from_error_kind(input: &[u8], kind: nom::error::ErrorKind) -> Self {
        Self::NomFailed(NomError {
            input: input[..input.len().min(256)].to_owned(),
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

pub fn render(_frame: &mut canvas::Frame, data: &[u8]) -> Result<(), Error> {
    let (data, _) = parse::header(data)?;
    let (_data, _chunk) = parse::chunk(data)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::{chunks::*, parse::*};
    use nom::{sequence::preceded, HexDisplay};
    use std::{error::Error, fmt::Write};

    const PNG: &[u8] = include_bytes!("../assets/xkcd.png");

    #[repr(transparent)]
    struct BytesPrinter([u8]);

    impl std::fmt::Debug for BytesPrinter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_char('\n')?;
            for i in 0..4 {
                f.write_str(&self.0.to_hex_from(8, i * 8))?;
            }
            Ok(())
        }
    }

    impl std::cmp::PartialEq for BytesPrinter {
        fn eq(&self, other: &Self) -> bool {
            &self.0 == &other.0
        }
    }

    impl From<&[u8]> for &BytesPrinter {
        fn from(value: &[u8]) -> Self {
            unsafe { std::mem::transmute(value) }
        }
    }

    macro_rules! assert_bytes {
        ($left:expr, $right:expr $(,)?) => {
            assert_eq!(
                <&BytesPrinter>::from($left as &[u8]),
                <&BytesPrinter>::from($right as &[u8]),
            )
        };
    }

    #[test]
    fn parse_header() -> Result<(), Box<dyn Error>> {
        let (_, result) = header(PNG)?;
        assert_bytes!(result, b"\x89PNG\r\n\x1A\x0A");
        Ok(())
    }

    #[test]
    fn parse_chunk() -> Result<(), Box<dyn Error>> {
        let (_, chunk) = preceded(header, chunk)(PNG)?;
        assert_eq!(
            chunk,
            Chunk::Ihdr(Ihdr {
                width: 293,
                height: 165,
                bit_depth: BitDepth::Eight,
                color_type: ColorType::Rgb,
                interlace: Interlace::None,
            })
        );
        Ok(())
    }
}

mod chunks;
mod error;
mod parse;

use iced::widget::canvas;

use chunks::Chunk;
use error::Error;

use self::chunks::Ihdr;

pub fn render(frame: &mut canvas::Frame, data: &[u8]) -> Result<(), Error> {
    let (data, _) = parse::header(data)?;
    let (_data, chunk) = parse::chunk(data)?;
    let Chunk::Ihdr(Ihdr {
        width,
        height,
        bit_depth,
        color_type,
        interlace,
    }) = chunk
    else {
        return Err(Error::MissingCritical("IHDR"));
    };

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
            let len = self.0.len().min(64);
            f.write_str(&self.0[..len].to_hex(8))
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
    fn parse_ihdr() -> Result<(), Box<dyn Error>> {
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

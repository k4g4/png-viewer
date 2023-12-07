#[derive(Debug, Clone, PartialEq)]
pub enum BitDepth {
    One,
    Two,
    Four,
    Eight,
    Sixteen,
}

impl TryFrom<u8> for BitDepth {
    type Error = super::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            4 => Ok(Self::Four),
            8 => Ok(Self::Eight),
            16 => Ok(Self::Sixteen),
            _ => Err(super::Error::InvalidBitDepth(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColorType {
    GrayScale,
    Rgb,
    Palette,
    GrayScaleAlpha,
    RgbAlpha,
}

impl TryFrom<u8> for ColorType {
    type Error = super::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::GrayScale),
            2 => Ok(Self::Rgb),
            3 => Ok(Self::Palette),
            4 => Ok(Self::GrayScaleAlpha),
            6 => Ok(Self::RgbAlpha),
            _ => Err(super::Error::InvalidColorType(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Interlace {
    Adam7,
    None,
}

impl TryFrom<u8> for Interlace {
    type Error = super::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Adam7),
            _ => Err(super::Error::InvalidInterlace(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Colors<'data>(&'data [u8]);

impl<'data> Colors<'data> {
    fn get(&self, index: usize) -> iced::Color {
        if let [r, g, b] = self.0[index * 3..][..3] {
            iced::Color::from_rgb8(r, g, b)
        } else {
            panic!(
                "index out of bounds: the len is {} but the index is {}",
                self.len(),
                index
            )
        }
    }

    fn len(&self) -> usize {
        self.0.len() / 3
    }
}

impl<'data> TryFrom<&[u8]> for Colors<'data> {
    type Error = super::Error;

    fn try_from(input: &[u8]) -> Result<Self, Self::Error> {
        if input.len() % 3 > 0 || input.len() > 256 * 3 {
            Err(super::Error::InvalidPaletteSize(input.len()))
        } else {
            Ok(Self(input))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Chunk<'data> {
    Ihdr {
        width: u32,
        height: u32,
        bit_depth: BitDepth,
        color_type: ColorType,
        interlace: Interlace,
    },
    Plte(Colors<'data>),
    Idat(&'data [u8]),
    Iend,
    Unknown,
}

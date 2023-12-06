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
pub enum Chunk {
    Ihdr {
        width: u32,
        height: u32,
        bit_depth: BitDepth,
        color_type: ColorType,
        interlace: Interlace,
    },
    Plte(Vec<iced::Color>),
    Idat,
    Iend,
    Unknown,
}

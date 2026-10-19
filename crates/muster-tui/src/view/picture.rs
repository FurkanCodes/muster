//! An agent's logo laid on the background of the line it is drawn on.
//!
//! A terminal draws a picture's transparent pixels in its own background,
//! not in the colour of the cells under it, and the second of the two cells
//! a logo takes is never drawn again (`CellDiffOption::Skip`), so on a
//! tinted line, the selected one or one that flashes, a logo with
//! transparent pixels would sit in a square of the plain background. Laid
//! on the line's colour instead, its own pixels blended over it, and in a
//! picture as wide as its two cells are, it covers them in the right
//! colour. The pictures are made here, as PNG files of no compression (a
//! few kilobytes, sent only when the line changes), and kept for the few
//! colours a screen has.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use muster_core::protocol::PixelSize;

/// How many pixels a logo is on each side.
pub(crate) const SIDE: u32 = 64;

/// The most laid pictures kept; past it they are let go and made again,
/// which a screen of a few colours never needs (`ARC-14`).
const KEPT: usize = 32;

/// What a logo is laid on: a colour, and the size of the picture, which is
/// as wide as its two cells are for their height.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Backdrop {
    pub(crate) colour: [u8; 3],
    pub(crate) canvas: (u32, u32),
}

impl Backdrop {
    /// `colour` behind two cells of `cell`, where the terminal says its
    /// cells' pixels; square where it does not.
    pub(crate) fn new(colour: [u8; 3], cell: Option<PixelSize>) -> Self {
        Self {
            colour,
            canvas: canvas(cell),
        }
    }
}

/// The size of a picture filling two cells of `cell` across and one down,
/// the logo's side its shorter one.
fn canvas(cell: Option<PixelSize>) -> (u32, u32) {
    let Some(cell) = cell.filter(|cell| cell.width > 0 && cell.height > 0) else {
        return (SIDE, SIDE);
    };
    let (wide, high) = (2 * u32::from(cell.width), u32::from(cell.height));
    let scaled = |long: u32, short: u32| ((SIDE * long + short / 2) / short).min(4 * SIDE);
    if wide >= high {
        (scaled(wide, high), SIDE)
    } else {
        (SIDE, scaled(high, wide))
    }
}

/// The picture of `name`, whose pixels are `rgba` (`SIDE` by `SIDE`), laid
/// on `backdrop`, as made by `make` once and kept.
pub(crate) fn kept(
    name: &'static str,
    rgba: &[u8],
    backdrop: Backdrop,
    make: impl FnOnce(&[u8]) -> String,
) -> String {
    static KEPT_PICTURES: LazyLock<Mutex<HashMap<(&'static str, Backdrop), String>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut kept = KEPT_PICTURES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(found) = kept.get(&(name, backdrop)) {
        return found.clone();
    }
    if kept.len() >= KEPT {
        kept.clear();
    }
    let made = make(&png(backdrop.canvas, &laid(rgba, backdrop)));
    kept.insert((name, backdrop), made.clone());
    made
}

/// `rgba`, `SIDE` pixels square, in the middle of a picture of
/// `backdrop`'s size and colour, blended over it: its pixels, red, green
/// and blue.
pub(crate) fn laid(rgba: &[u8], backdrop: Backdrop) -> Vec<u8> {
    let (width, height) = backdrop.canvas;
    let (left, top) = ((width - SIDE) / 2, (height - SIDE) / 2);
    let mut out = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let inside = (left..left + SIDE).contains(&x) && (top..top + SIDE).contains(&y);
            let pixel = inside.then(|| {
                let at = (((y - top) * SIDE + (x - left)) * 4) as usize;
                rgba.get(at..at + 4)
            });
            match pixel.flatten() {
                Some(&[red, green, blue, alpha]) => {
                    let blend = |over: u8, under: u8| {
                        let (over, under, alpha) =
                            (u16::from(over), u16::from(under), u16::from(alpha));
                        u8::try_from((over * alpha + under * (255 - alpha) + 127) / 255)
                            .unwrap_or(u8::MAX)
                    };
                    let [r, g, b] = backdrop.colour;
                    out.extend([blend(red, r), blend(green, g), blend(blue, b)]);
                }
                _ => out.extend(backdrop.colour),
            }
        }
    }
    out
}

/// `rgb`, `width` by `height` pixels, as a PNG file, its image data in
/// stored (uncompressed) deflate blocks.
pub(crate) fn png((width, height): (u32, u32), rgb: &[u8]) -> Vec<u8> {
    let row = (width * 3) as usize;
    let mut raw = Vec::with_capacity(rgb.len() + height as usize);
    for line in rgb.chunks(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    // Eight bits, red green blue, deflate, no filter, not interlaced.
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, *b"IHDR", &header);
    chunk(&mut out, *b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, *b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], body: &[u8]) {
    out.extend(u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
    let start = out.len();
    out.extend(kind);
    out.extend(body);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

/// `data` as a zlib stream of stored blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 65_535;
    let mut out = vec![0x78, 0x01];
    let blocks = data.chunks(BLOCK).collect::<Vec<_>>();
    if blocks.is_empty() {
        out.extend([1, 0, 0, 0xff, 0xff]);
    }
    for (at, block) in blocks.iter().enumerate() {
        out.push(u8::from(at + 1 == blocks.len()));
        let length = u16::try_from(block.len()).unwrap_or(u16::MAX);
        out.extend(length.to_le_bytes());
        out.extend((!length).to_le_bytes());
        out.extend_from_slice(block);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    out.extend(((b << 16) | a).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    static TABLE: LazyLock<[u32; 256]> = LazyLock::new(|| {
        let mut table = [0; 256];
        for (n, entry) in (0_u32..).zip(table.iter_mut()) {
            let mut c = n;
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *entry = c;
        }
        table
    });
    !data.iter().fold(!0_u32, |c, byte| {
        TABLE[((c ^ u32::from(*byte)) & 0xff) as usize] ^ (c >> 8)
    })
}

/// What a test reads back of a PNG file [`png`] made: its size and pixels.
#[cfg(test)]
pub(crate) fn read_png(file: &[u8]) -> ((u32, u32), Vec<u8>) {
    assert_eq!(&file[..8], b"\x89PNG\r\n\x1a\n");
    let (mut at, mut size, mut data) = (8, (0, 0), Vec::new());
    while at < file.len() {
        let length = u32::from_be_bytes(file[at..at + 4].try_into().unwrap()) as usize;
        let kind = &file[at + 4..at + 8];
        let body = &file[at + 8..at + 8 + length];
        let crc = u32::from_be_bytes(file[at + 8 + length..at + 12 + length].try_into().unwrap());
        assert_eq!(
            crc,
            crc32(&file[at + 4..at + 8 + length]),
            "each chunk's CRC"
        );
        match kind {
            b"IHDR" => {
                size = (
                    u32::from_be_bytes(body[..4].try_into().unwrap()),
                    u32::from_be_bytes(body[4..8].try_into().unwrap()),
                );
                assert_eq!(&body[8..], [8, 2, 0, 0, 0]);
            }
            b"IDAT" => data.extend_from_slice(body),
            _ => {}
        }
        at += 12 + length;
    }
    // Stored blocks: a byte saying last or not, the length, its
    // complement, then the bytes.
    let mut raw = Vec::new();
    let mut at = 2;
    loop {
        let last = data[at] & 1 == 1;
        let length = usize::from(u16::from_le_bytes([data[at + 1], data[at + 2]]));
        assert_eq!(
            u16::from_le_bytes([data[at + 3], data[at + 4]]),
            !u16::try_from(length).unwrap()
        );
        raw.extend_from_slice(&data[at + 5..at + 5 + length]);
        at += 5 + length;
        if last {
            break;
        }
    }
    let row = (size.0 * 3) as usize + 1;
    let pixels = raw
        .chunks(row)
        .flat_map(|line| {
            assert_eq!(line[0], 0, "no filter");
            line[1..].to_vec()
        })
        .collect();
    (size, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_as_wide_as_its_two_cells_are_for_their_height() {
        assert_eq!(canvas(None), (64, 64));
        // Cells 8 by 18: two across are 16 by 18, a little taller.
        let tall = PixelSize {
            width: 8,
            height: 18,
        };
        assert_eq!(canvas(Some(tall)), (64, 72));
        let wide = PixelSize {
            width: 10,
            height: 16,
        };
        assert_eq!(canvas(Some(wide)), (80, 64));
        let none = PixelSize {
            width: 0,
            height: 16,
        };
        assert_eq!(canvas(Some(none)), (64, 64));
    }

    #[test]
    fn a_logo_is_blended_over_its_backdrop_in_the_middle() {
        // A logo opaque red in its first pixel, half see-through green in
        // its second, and clear in the rest.
        let mut rgba = vec![0; (SIDE * SIDE * 4) as usize];
        rgba[..8].copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 128]);
        let backdrop = Backdrop {
            colour: [10, 20, 30],
            canvas: (66, 64),
        };
        let out = laid(&rgba, backdrop);
        assert_eq!(out.len(), 66 * 64 * 3);
        // A column of the backdrop either side, then the logo.
        assert_eq!(&out[..3], [10, 20, 30]);
        assert_eq!(&out[3..6], [255, 0, 0]);
        assert_eq!(&out[6..9], [5, 138, 15]);
        assert_eq!(&out[9..12], [10, 20, 30]);
        assert_eq!(&out[out.len() - 3..], [10, 20, 30]);
    }

    #[test]
    fn the_png_reads_back_as_written() {
        let rgb: Vec<u8> = (0..(3 * 2 * 3)).map(|n| n * 10).collect();
        let file = png((3, 2), &rgb);
        assert_eq!(read_png(&file), ((3, 2), rgb));
        // Larger than one stored block.
        let big = vec![7; (200 * 200 * 3) as usize];
        assert_eq!(read_png(&png((200, 200), &big)), ((200, 200), big));
    }

    #[test]
    fn crc_and_adler_are_the_standard_ones() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        let stream = zlib_stored(b"Wikipedia");
        assert_eq!(&stream[stream.len() - 4..], 0x11e6_0398_u32.to_be_bytes());
    }

    #[test]
    fn a_laid_picture_is_made_once_and_kept() {
        let rgba = vec![0; (SIDE * SIDE * 4) as usize];
        let backdrop = Backdrop::new([1, 2, 3], None);
        let mut made = 0;
        let first = kept("test", &rgba, backdrop, |png| {
            made += 1;
            format!("{}", png.len())
        });
        let again = kept("test", &rgba, backdrop, |_| unreachable!("kept"));
        assert_eq!((first, made), (again, 1));
    }
}

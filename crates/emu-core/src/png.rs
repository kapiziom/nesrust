//! A tiny PNG encoder for screenshots (uncompressed deflate blocks, so no compression crate is needed), and a
//! helper that lays several frames out on one contact sheet.

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Encodes 8-bit RGBA pixels (rows top to bottom) as a PNG file.
pub fn encode_rgba(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), width * height * 4, "the pixel data does not match the size");

    let mut raw = Vec::with_capacity((width * 4 + 1) * height);
    for row in rgba.chunks_exact(width * 4) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }

    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(blocks.peek().is_none() as u8); // BFINAL, BTYPE = stored
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut header = Vec::new();
    header.extend_from_slice(&(width as u32).to_be_bytes());
    header.extend_from_slice(&(height as u32).to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits per channel, RGBA

    let mut png = SIGNATURE.to_vec();
    for (kind, data) in [(b"IHDR", &header[..]), (b"IDAT", &zlib[..]), (b"IEND", &[][..])] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }
    png
}

/// Places equally sized RGBA frames in a grid of `columns` (with a 2 pixel gap) and returns the sheet's size and
/// pixels. Unused cells stay black.
pub fn contact_sheet(frames: &[&[u8]], width: usize, height: usize, columns: usize) -> (usize, usize, Vec<u8>) {
    const GAP: usize = 2;
    let columns = columns.clamp(1, frames.len().max(1));
    let rows = frames.len().div_ceil(columns).max(1);
    let sheet_width = columns * width + (columns - 1) * GAP;
    let sheet_height = rows * height + (rows - 1) * GAP;
    let mut sheet = vec![0u8; sheet_width * sheet_height * 4];
    for pixel in sheet.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }

    for (index, frame) in frames.iter().enumerate() {
        let left = (index % columns) * (width + GAP);
        let top = (index / columns) * (height + GAP);
        for (y, row) in frame.chunks_exact(width * 4).enumerate() {
            let start = ((top + y) * sheet_width + left) * 4;
            sheet[start..start + width * 4].copy_from_slice(row);
        }
    }

    (sheet_width, sheet_height, sheet)
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the chunks of a PNG: (type, data), checking the signature and every CRC.
    fn chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&png[..8], &SIGNATURE);
        let mut position = 8;
        let mut found = Vec::new();
        while position < png.len() {
            let length = u32::from_be_bytes(png[position..position + 4].try_into().unwrap()) as usize;
            let kind: [u8; 4] = png[position + 4..position + 8].try_into().unwrap();
            let data = png[position + 8..position + 8 + length].to_vec();
            let crc = u32::from_be_bytes(png[position + 8 + length..position + 12 + length].try_into().unwrap());
            assert_eq!(crc, crc32(&png[position + 4..position + 8 + length]), "CRC of {kind:?}");
            found.push((kind, data));
            position += 12 + length;
        }
        found
    }

    /// Undoes the stored-block deflate stream of `encode_rgba`.
    fn inflate_stored(zlib: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut position = 2;
        loop {
            let last = zlib[position] & 1 != 0;
            let length = u16::from_le_bytes([zlib[position + 1], zlib[position + 2]]) as usize;
            assert_eq!(u16::from_le_bytes([zlib[position + 3], zlib[position + 4]]), !(length as u16));
            out.extend_from_slice(&zlib[position + 5..position + 5 + length]);
            position += 5 + length;
            if last {
                break;
            }
        }
        assert_eq!(u32::from_be_bytes(zlib[position..position + 4].try_into().unwrap()), adler32(&out));
        out
    }

    #[test]
    fn the_png_has_valid_chunks_and_decodes_to_the_same_pixels() {
        // large enough for several stored blocks
        let (width, height) = (200, 150);
        let rgba: Vec<u8> = (0..width * height * 4).map(|i| (i * 7 % 251) as u8).collect();

        let png = encode_rgba(width, height, &rgba);
        let found = chunks(&png);

        let kinds: Vec<_> = found.iter().map(|(kind, _)| kind).collect();
        assert_eq!(kinds, [b"IHDR", b"IDAT", b"IEND"]);
        assert_eq!(&found[0].1[..8], &[0, 0, 0, 200, 0, 0, 0, 150]);
        let raw = inflate_stored(&found[1].1);
        assert_eq!(raw.len(), (width * 4 + 1) * height);
        let pixels: Vec<u8> = raw.chunks_exact(width * 4 + 1).flat_map(|row| row[1..].iter().copied()).collect();
        assert_eq!(pixels, rgba);
    }

    #[test]
    fn a_contact_sheet_lays_frames_out_in_a_grid() {
        let red = vec![255, 0, 0, 255, 255, 0, 0, 255]; // 2x1
        let green = vec![0, 255, 0, 255, 0, 255, 0, 255];
        let blue = vec![0, 0, 255, 255, 0, 0, 255, 255];

        let (width, height, sheet) = contact_sheet(&[&red, &green, &blue], 2, 1, 2);

        assert_eq!((width, height), (2 + 2 + 2, 1 + 2 + 1));
        assert_eq!(&sheet[0..4], &[255, 0, 0, 255], "first frame top left");
        assert_eq!(&sheet[(2 + 2) * 4..(2 + 2) * 4 + 4], &[0, 255, 0, 255], "second frame after the gap");
        let third_row = (1 + 2) * width * 4;
        assert_eq!(&sheet[third_row..third_row + 4], &[0, 0, 255, 255], "third frame starts the second row");
        let unused_cell = third_row + (2 + 2) * 4;
        assert_eq!(&sheet[unused_cell..unused_cell + 4], &[0, 0, 0, 255], "unused cells are black");
    }
}

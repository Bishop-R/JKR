//! A decoder for id Software's RoQ video, enough for the classic main page's
//! `video/ja01` logo (`gfx/menus/videologo`'s `videoMap`).
//!
//! A RoQ file is a run of chunks (`u16` id, `u32` size, `u16` argument):
//! a signature with the frame rate, an info chunk with the size, then per
//! frame a codebook (`0x1002`) and a vector-quantised picture (`0x1011`);
//! audio chunks are skipped. The picture is coded in 16x16 macroblocks, each
//! split into four 8x8 blocks and those into 4x4 blocks by two-bit codes:
//! keep the previous frame's block, copy a moved block of it, or paint
//! codebook cells (2x2 YCbCr cells, doubled to 4x4 at the 8x8 level). The
//! layout follows the reference decoders (id's `cl_cin.cpp`, FFmpeg's
//! `roqvideodec.c`); JPEG frames (`0x1012`), which these logos do not use,
//! are skipped.
//!
//! Codebook cells are converted to RGBA when they are read, so frames are
//! decoded straight into an RGBA buffer ready to upload. Decoding allocates
//! nothing after [`RoqDecoder::new`].

const SIGNATURE: u16 = 0x1084;
const INFO: u16 = 0x1001;
const CODEBOOK: u16 = 0x1002;
const VECTORS: u16 = 0x1011;
const HEADER_BYTES: usize = 8;
/// Largest picture accepted; retail's logo is 512x512.
const MAX_SIDE: u32 = 2_048;

/// Two-bit block codes.
const MOT: u16 = 0;
const FCC: u16 = 1;
const SLD: u16 = 2;

/// A frame-by-frame RoQ decoder over a whole file in memory.
pub(crate) struct RoqDecoder<'a> {
    data: &'a [u8],
    /// Offset of the first chunk after the signature, where a loop restarts.
    start: usize,
    /// Offset of the next chunk.
    position: usize,
    width: u32,
    height: u32,
    fps: u32,
    /// 2x2 cells as RGBA, row by row.
    cells: Box<[[u8; 16]; 256]>,
    /// 4x4 cells as four 2x2 cell indices (top-left, top-right, bottom-left,
    /// bottom-right).
    quads: Box<[[u8; 4]; 256]>,
    frame: Vec<u8>,
    previous: Vec<u8>,
}

/// One chunk header.
#[derive(Clone, Copy)]
struct Chunk {
    id: u16,
    size: usize,
    argument: u16,
    body: usize,
}

impl<'a> RoqDecoder<'a> {
    /// Read the signature and picture size of `data`; `None` if it is not a
    /// RoQ file JKR can show.
    pub(crate) fn new(data: &'a [u8]) -> Option<Self> {
        let signature = chunk_at(data, 0)?;
        if signature.id != SIGNATURE {
            return None;
        }
        let start = HEADER_BYTES;
        let mut position = start;
        let (width, height) = loop {
            let chunk = chunk_at(data, position)?;
            if chunk.id == INFO {
                let body = data.get(chunk.body..chunk.body + 4)?;
                break (
                    u32::from(u16::from_le_bytes([body[0], body[1]])),
                    u32::from(u16::from_le_bytes([body[2], body[3]])),
                );
            }
            position = chunk.body.checked_add(chunk.size)?;
        };
        let fits = |side: u32| side > 0 && side <= MAX_SIDE && side % 16 == 0;
        if !fits(width) || !fits(height) {
            return None;
        }
        let pixels = (width * height * 4) as usize;
        Some(Self {
            data,
            start,
            position: start,
            width,
            height,
            fps: match signature.argument {
                0 => 30,
                fps => u32::from(fps),
            },
            cells: Box::new([[0; 16]; 256]),
            quads: Box::new([[0; 4]; 256]),
            frame: black(pixels),
            previous: black(pixels),
        })
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    /// Frames per second.
    pub(crate) fn fps(&self) -> u32 {
        self.fps
    }

    /// The last decoded frame, RGBA rows top to bottom.
    pub(crate) fn frame(&self) -> &[u8] {
        &self.frame
    }

    /// Decode the next frame, starting over from black after the last one
    /// (retail loops the logo). False when the file holds no frame at all.
    pub(crate) fn next_frame(&mut self) -> bool {
        let mut restarted = false;
        loop {
            let Some(chunk) = chunk_at(self.data, self.position) else {
                if restarted {
                    return false;
                }
                restarted = true;
                self.position = self.start;
                self.frame.fill(0);
                self.previous.fill(0);
                for pixel in self.frame.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
                continue;
            };
            let Some(end) = chunk.body.checked_add(chunk.size) else {
                self.position = self.data.len();
                continue;
            };
            self.position = end;
            match chunk.id {
                CODEBOOK => self.read_codebook(chunk),
                VECTORS => {
                    self.previous.copy_from_slice(&self.frame);
                    self.decode_vectors(chunk);
                    return true;
                }
                _ => {}
            }
        }
    }

    fn read_codebook(&mut self, chunk: Chunk) {
        let Some(body) = self.data.get(chunk.body..chunk.body + chunk.size) else {
            return;
        };
        let cells = match usize::from(chunk.argument >> 8) {
            0 => 256,
            count => count,
        };
        let quads = match usize::from(chunk.argument & 0xff) {
            0 if cells * 6 < chunk.size => 256,
            count => count,
        };
        let mut bytes = body.iter().copied();
        for cell in self.cells.iter_mut().take(cells) {
            let mut take = || bytes.next().unwrap_or(0);
            let luma = [take(), take(), take(), take()];
            let (blue, red) = (take(), take());
            for (pixel, y) in luma.into_iter().enumerate() {
                cell[pixel * 4..pixel * 4 + 4].copy_from_slice(&rgba(y, blue, red));
            }
        }
        for quad in self.quads.iter_mut().take(quads) {
            for index in quad.iter_mut() {
                *index = bytes.next().unwrap_or(0);
            }
        }
    }

    fn decode_vectors(&mut self, chunk: Chunk) {
        let end = (chunk.body + chunk.size).min(self.data.len());
        let mut reader = Reader {
            data: &self.data[..end],
            position: chunk.body,
            flags: 0,
            remaining: 0,
        };
        // The chunk argument is the mean motion, signed bytes x then y.
        let mean = [
            i32::from((chunk.argument >> 8) as u8 as i8),
            i32::from(chunk.argument as u8 as i8),
        ];
        let (mut x, mut y) = (0, 0);
        while reader.position < end && y < self.height {
            for (block_x, block_y) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
                if reader.position >= end {
                    break;
                }
                let (bx, by) = (x + block_x, y + block_y);
                match reader.code() {
                    MOT => {}
                    FCC => {
                        let shift = reader.byte();
                        self.copy_moved(bx, by, 8, motion(shift, mean));
                    }
                    SLD => {
                        let quad = self.quads[usize::from(reader.byte())];
                        for (corner, cell) in quad.into_iter().enumerate() {
                            let (dx, dy) = quadrant(corner, 4);
                            self.paint_doubled(bx + dx, by + dy, cell);
                        }
                    }
                    _ => {
                        for corner in 0..4 {
                            let (dx, dy) = quadrant(corner, 4);
                            self.decode_four(&mut reader, bx + dx, by + dy, mean);
                        }
                    }
                }
            }
            x += 16;
            if x >= self.width {
                x = 0;
                y += 16;
            }
        }
    }

    /// One 4x4 block of an 8x8 block coded as four.
    fn decode_four(&mut self, reader: &mut Reader<'_>, x: u32, y: u32, mean: [i32; 2]) {
        match reader.code() {
            MOT => {}
            FCC => {
                let shift = reader.byte();
                self.copy_moved(x, y, 4, motion(shift, mean));
            }
            SLD => {
                let quad = self.quads[usize::from(reader.byte())];
                for (corner, cell) in quad.into_iter().enumerate() {
                    let (dx, dy) = quadrant(corner, 2);
                    self.paint(x + dx, y + dy, cell);
                }
            }
            _ => {
                for corner in 0..4 {
                    let cell = reader.byte();
                    let (dx, dy) = quadrant(corner, 2);
                    self.paint(x + dx, y + dy, cell);
                }
            }
        }
    }

    /// Copy the previous frame's `size` square at the block moved by
    /// `delta` to `(x, y)`; a source outside the picture is skipped, as the
    /// reference decoders reject it.
    fn copy_moved(&mut self, x: u32, y: u32, size: u32, delta: [i32; 2]) {
        let source_x = x as i32 + delta[0];
        let source_y = y as i32 + delta[1];
        if source_x < 0
            || source_y < 0
            || source_x + size as i32 > self.width as i32
            || source_y + size as i32 > self.height as i32
        {
            return;
        }
        let row_bytes = (size * 4) as usize;
        for row in 0..size {
            let from = self.offset(source_x as u32, source_y as u32 + row);
            let to = self.offset(x, y + row);
            self.frame[to..to + row_bytes].copy_from_slice(&self.previous[from..from + row_bytes]);
        }
    }

    /// Paint 2x2 cell `cell` at `(x, y)`.
    fn paint(&mut self, x: u32, y: u32, cell: u8) {
        let cell = self.cells[usize::from(cell)];
        for row in 0..2 {
            let to = self.offset(x, y + row);
            let from = (row * 8) as usize;
            self.frame[to..to + 8].copy_from_slice(&cell[from..from + 8]);
        }
    }

    /// Paint 2x2 cell `cell` at `(x, y)` with every pixel doubled, 4x4.
    fn paint_doubled(&mut self, x: u32, y: u32, cell: u8) {
        let cell = self.cells[usize::from(cell)];
        for row in 0..4 {
            let to = self.offset(x, y + row);
            for column in 0..4 {
                let pixel = ((row / 2) * 2 + column / 2) as usize * 4;
                let at = to + column as usize * 4;
                self.frame[at..at + 4].copy_from_slice(&cell[pixel..pixel + 4]);
            }
        }
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        ((y * self.width + x) * 4) as usize
    }
}

/// The two-bit codes and bytes of one picture chunk.
struct Reader<'a> {
    data: &'a [u8],
    position: usize,
    flags: u16,
    remaining: u8,
}

impl Reader<'_> {
    fn byte(&mut self) -> u8 {
        let byte = self.data.get(self.position).copied().unwrap_or(0);
        self.position += 1;
        byte
    }

    /// The next code: codes come eight to a little-endian word, most
    /// significant pair first.
    fn code(&mut self) -> u16 {
        if self.remaining == 0 {
            self.flags = u16::from_le_bytes([self.byte(), self.byte()]);
            self.remaining = 8;
        }
        self.remaining -= 1;
        (self.flags >> (u16::from(self.remaining) * 2)) & 3
    }
}

/// Where a block's source lies: `8 - nibble - mean` on each axis, the high
/// nibble for x.
fn motion(shift: u8, mean: [i32; 2]) -> [i32; 2] {
    [
        8 - i32::from(shift >> 4) - mean[0],
        8 - i32::from(shift & 0x0f) - mean[1],
    ]
}

/// Offset of quadrant `corner` (top-left, top-right, bottom-left,
/// bottom-right) of a block whose quadrants are `half` wide.
fn quadrant(corner: usize, half: u32) -> (u32, u32) {
    (
        if corner & 1 != 0 { half } else { 0 },
        if corner & 2 != 0 { half } else { 0 },
    )
}

fn chunk_at(data: &[u8], position: usize) -> Option<Chunk> {
    let header = data.get(position..position.checked_add(HEADER_BYTES)?)?;
    Some(Chunk {
        id: u16::from_le_bytes([header[0], header[1]]),
        size: u32::from_le_bytes([header[2], header[3], header[4], header[5]]) as usize,
        argument: u16::from_le_bytes([header[6], header[7]]),
        body: position + HEADER_BYTES,
    })
}

/// A black, opaque RGBA picture of `bytes` bytes.
fn black(bytes: usize) -> Vec<u8> {
    let mut pixels = vec![0_u8; bytes];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    pixels
}

/// Full-range YCbCr to RGBA, as the reference decoders convert.
fn rgba(y: u8, blue: u8, red: u8) -> [u8; 4] {
    let y = f32::from(y);
    let cb = f32::from(blue) - 128.0;
    let cr = f32::from(red) - 128.0;
    let clamp = |value: f32| value.round().clamp(0.0, 255.0) as u8;
    [
        clamp(y + 1.402 * cr),
        clamp(y - 0.344_136 * cb - 0.714_136 * cr),
        clamp(y + 1.772 * cb),
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(out: &mut Vec<u8>, id: u16, argument: u16, body: &[u8]) {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&argument.to_le_bytes());
        out.extend_from_slice(body);
    }

    /// A 16x16, 30 fps file: info, then the chunks `frames` adds.
    fn file(frames: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&SIGNATURE.to_le_bytes());
        out.extend_from_slice(&u32::MAX.to_le_bytes());
        out.extend_from_slice(&30_u16.to_le_bytes());
        chunk(&mut out, INFO, 0, &[16, 0, 16, 0, 8, 0, 4, 0]);
        frames(&mut out);
        out
    }

    /// Codebook: cell 0 grey 0, cell 1 white, cell 2 a top-left white
    /// corner; quad 0 all cell 1, quad 1 cells 0 1 2 1.
    fn codebook(out: &mut Vec<u8>) {
        let mut body = Vec::new();
        body.extend_from_slice(&[0, 0, 0, 0, 128, 128]);
        body.extend_from_slice(&[255, 255, 255, 255, 128, 128]);
        body.extend_from_slice(&[255, 0, 0, 0, 128, 128]);
        body.extend_from_slice(&[1, 1, 1, 1]);
        body.extend_from_slice(&[0, 1, 2, 1]);
        chunk(out, CODEBOOK, (3 << 8) | 2, &body);
    }

    fn pixel(decoder: &RoqDecoder<'_>, x: u32, y: u32) -> [u8; 4] {
        let at = decoder.offset(x, y);
        decoder.frame()[at..at + 4].try_into().expect("pixel")
    }

    #[test]
    fn reads_the_header() {
        let data = file(|_| {});
        let decoder = RoqDecoder::new(&data).expect("roq");
        assert_eq!(
            (decoder.width(), decoder.height(), decoder.fps()),
            (16, 16, 30)
        );
        assert!(RoqDecoder::new(&data[2..]).is_none());
        assert!(RoqDecoder::new(b"not a video").is_none());
    }

    #[test]
    fn paints_codebook_cells_at_both_sizes() {
        let data = file(|out| {
            codebook(out);
            // Codes, most significant pair first, eight to a word: SLD (quad
            // 0 doubled), CCC whose four 4x4 codes follow at once (SLD quad
            // 1, CCC cells 2 1 0 1, MOT, MOT), then MOT, MOT.
            let codes: u16 = (SLD << 14) | (3 << 12) | (SLD << 10) | (3 << 8);
            let mut body = Vec::new();
            body.extend_from_slice(&codes.to_le_bytes());
            body.push(0); // SLD: quad 0
            body.push(1); // inner SLD: quad 1
            body.extend_from_slice(&[2, 1, 0, 1]); // inner CCC cells
            chunk(out, VECTORS, 0, &body);
        });
        let mut decoder = RoqDecoder::new(&data).expect("roq");
        assert!(decoder.next_frame());
        // The first 8x8 block is white throughout.
        assert_eq!(pixel(&decoder, 0, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&decoder, 7, 7), [255, 255, 255, 255]);
        // The second: its top-left 4x4 is quad 1 (black, white / white corner, white).
        assert_eq!(pixel(&decoder, 8, 0), [0, 0, 0, 255]);
        assert_eq!(pixel(&decoder, 10, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&decoder, 8, 2), [255, 255, 255, 255]);
        assert_eq!(pixel(&decoder, 9, 2), [0, 0, 0, 255]);
        // Its top-right 4x4 is cells 2 1 0 1 at 2x2.
        assert_eq!(pixel(&decoder, 12, 0), [255, 255, 255, 255]);
        assert_eq!(pixel(&decoder, 13, 0), [0, 0, 0, 255]);
        assert_eq!(pixel(&decoder, 12, 2), [0, 0, 0, 255]);
        // The kept blocks stay black.
        assert_eq!(pixel(&decoder, 0, 8), [0, 0, 0, 255]);
    }

    #[test]
    fn copies_moved_blocks_and_loops() {
        let data = file(|out| {
            codebook(out);
            // Frame 1: the top-left 8x8 white, the rest kept (black).
            let mut body = Vec::new();
            body.extend_from_slice(&(SLD << 14).to_le_bytes());
            body.push(0);
            chunk(out, VECTORS, 0, &body);
            // Frame 2: the top-right 8x8 copies the block 8 units to its
            // left: x = 8 - nibble 0 - mean 16 = -8, y = 8 - 8 - 0 = 0.
            let mut body = Vec::new();
            body.extend_from_slice(&((MOT << 14) | (FCC << 12)).to_le_bytes());
            body.push(0x08);
            chunk(out, VECTORS, 16 << 8, &body);
        });
        let mut decoder = RoqDecoder::new(&data).expect("roq");
        assert!(decoder.next_frame());
        assert_eq!(pixel(&decoder, 12, 0), [0, 0, 0, 255]);
        assert!(decoder.next_frame());
        assert_eq!(pixel(&decoder, 3, 3), [255, 255, 255, 255]);
        assert_eq!(pixel(&decoder, 12, 3), [255, 255, 255, 255]);
        assert_eq!(pixel(&decoder, 12, 12), [0, 0, 0, 255]);
        // The third call starts over from black.
        assert!(decoder.next_frame());
        assert_eq!(pixel(&decoder, 12, 3), [0, 0, 0, 255]);
        assert_eq!(pixel(&decoder, 3, 3), [255, 255, 255, 255]);
    }

    #[test]
    fn a_file_without_frames_reports_none() {
        let data = file(codebook);
        let mut decoder = RoqDecoder::new(&data).expect("roq");
        assert!(!decoder.next_frame());
    }

    #[test]
    fn converts_full_range_colour() {
        assert_eq!(rgba(128, 128, 128), [128, 128, 128, 255]);
        assert_eq!(rgba(0, 128, 255), [178, 0, 0, 255]);
        assert_eq!(rgba(255, 0, 128)[2], 28);
    }
}

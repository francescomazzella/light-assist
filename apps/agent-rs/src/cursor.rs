//! The shape of the sharer's mouse cursor.
//!
//! A helper who is controlling sees their own pointer, not the sharer's cursor, which is left out of
//! the picture. So that it still turns into a hand over a link or arrows on a window's edge, the
//! shape is read here and the helper page gives its own pointer the same one: by name for the shapes
//! every system has, and as a picture for those a program drew for itself.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

/// Larger cursors than this are not sent; browsers do not show them either.
const MAX_SIDE: u32 = 128;
/// Pictures remembered before starting over, for a program that keeps making new cursors.
const MAX_REMEMBERED: usize = 256;

/// What the helper page is told when the cursor changes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    /// The name browsers know the shape by (the CSS `cursor` keyword), or the plain arrow's when it has none.
    pub shape: &'static str,
    /// Names a drawn cursor's picture, which is sent only the first time: the helper page keeps it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The picture, as a PNG in a `data:` address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The point of the picture that does the pointing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotspot: Option<(u32, u32)>,
}

struct Drawn {
    id: String,
    image: String,
    hotspot: (u32, u32),
}

/// Follows the cursor for one helper, remembering which pictures that helper already has.
#[derive(Default)]
pub struct Cursors {
    current: Option<isize>,
    drawn: HashMap<isize, Option<Drawn>>,
    sent: HashSet<String>,
}

impl Cursors {
    /// Forgets what was last told, so that the next look tells it again.
    pub fn forget_current(&mut self) {
        self.current = None;
    }

    /// The cursor's shape, if it changed since the last look.
    pub fn changed(&mut self) -> Option<Shape> {
        let handle = platform::current()?;
        if self.current == Some(handle) {
            return None;
        }
        self.current = Some(handle);

        if let Some(shape) = platform::name(handle) {
            return Some(Shape { shape, id: None, image: None, hotspot: None });
        }
        if self.drawn.len() >= MAX_REMEMBERED {
            self.drawn.clear();
        }
        let drawn = self.drawn.entry(handle).or_insert_with(|| {
            platform::picture(handle).map(|(width, height, hotspot, rgba)| Drawn {
                id: format!("{:016x}", fnv1a(&rgba, (width, height), hotspot)),
                image: format!("data:image/png;base64,{}", base64(&png(width, height, &rgba))),
                hotspot,
            })
        });
        Some(match drawn {
            Some(drawn) => {
                // The picture travels once; after that its name is enough.
                let image = self.sent.insert(drawn.id.clone()).then(|| drawn.image.clone());
                Shape { shape: "default", id: Some(drawn.id.clone()), image, hotspot: Some(drawn.hotspot) }
            }
            None => Shape { shape: "default", id: None, image: None, hotspot: None },
        })
    }
}

fn fnv1a(pixels: &[u8], size: (u32, u32), hotspot: (u32, u32)) -> u64 {
    let header = [size.0, size.1, hotspot.0, hotspot.1].map(u32::to_le_bytes);
    header.iter().flatten().chain(pixels).fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3))
}

fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for &byte in chunks.iter().copied().flatten() {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (!(crc & 1)).wrapping_add(1));
        }
    }
    !crc
}

/// A PNG of 8-bit RGBA pixels. Not compressed: a cursor is a few kilobytes and is sent once.
fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
    }

    // Each row starts with the number of the filter applied to it: none.
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks_exact(row) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    // A zlib stream of stored blocks, closed by the Adler-32 checksum of what they hold.
    let mut packed = vec![0x78, 0x01];
    let mut blocks = raw.chunks(0xffff).peekable();
    while let Some(block) = blocks.next() {
        packed.push(blocks.peek().is_none() as u8);
        packed.extend_from_slice(&(block.len() as u16).to_le_bytes());
        packed.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        packed.extend_from_slice(block);
    }
    let (a, b) = raw.iter().fold((1u32, 0u32), |(a, b), &byte| ((a + byte as u32) % 65521, (b + a + byte as u32) % 65521));
    packed.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &packed);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let word = (group[0] as u32) << 16 | (*group.get(1).unwrap_or(&0) as u32) << 8 | *group.get(2).unwrap_or(&0) as u32;
        for position in 0..4 {
            let shown = position <= group.len();
            out.push(if shown { ALPHABET[(word >> (18 - 6 * position) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[cfg(windows)]
mod platform {
    use std::sync::OnceLock;

    use windows::{
        core::PCWSTR,
        Win32::{
            Graphics::Gdi::*,
            UI::WindowsAndMessaging::*,
        },
    };

    use super::MAX_SIDE;

    const NAMED: [(PCWSTR, &str); 13] = [
        (IDC_ARROW, "default"),
        (IDC_IBEAM, "text"),
        (IDC_HAND, "pointer"),
        (IDC_SIZEWE, "ew-resize"),
        (IDC_SIZENS, "ns-resize"),
        (IDC_SIZENWSE, "nwse-resize"),
        (IDC_SIZENESW, "nesw-resize"),
        (IDC_SIZEALL, "move"),
        (IDC_NO, "not-allowed"),
        (IDC_WAIT, "wait"),
        (IDC_APPSTARTING, "progress"),
        (IDC_CROSS, "crosshair"),
        (IDC_HELP, "help"),
    ];

    /// The cursor in use on the sharer's screen right now, by its handle.
    pub fn current() -> Option<isize> {
        let mut info = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        unsafe { GetCursorInfo(&mut info) }.ok()?;
        // No cursor at all, as while typing in some programs: the helper keeps the one they had.
        (!info.hCursor.is_invalid()).then_some(info.hCursor.0 as isize)
    }

    /// The name of one of Windows' own cursors.
    pub fn name(handle: isize) -> Option<&'static str> {
        // Windows hands every program the same handles for its own cursors, so they can be told apart by
        // comparing. Kept as numbers, which may be shared between threads where the handles may not.
        static HANDLES: OnceLock<Vec<(isize, &'static str)>> = OnceLock::new();
        let handles = HANDLES.get_or_init(|| {
            NAMED
                .iter()
                .filter_map(|&(id, name)| unsafe { LoadCursorW(None, id) }.ok().map(|cursor| (cursor.0 as isize, name)))
                .collect()
        });
        handles.iter().find(|(known, _)| *known == handle).map(|(_, name)| *name)
    }

    /// The pixels of a bitmap, top row first, four bytes each in the order blue, green, red, alpha.
    unsafe fn pixels(dc: HDC, bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
        let mut described = BITMAP::default();
        if GetObjectW(bitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut described as *mut _ as *mut _)) == 0 {
            return None;
        }
        let (width, height) = (described.bmWidth as u32, described.bmHeight as u32);
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE * 2 {
            return None;
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // Negative: the first row is the top one.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut data = vec![0u8; (width * height * 4) as usize];
        let read = GetDIBits(dc, bitmap, 0, height, Some(data.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        (read as u32 == height).then_some((width, height, data))
    }

    /// A drawn cursor's picture: its size, the point that does the pointing, and its pixels as RGBA.
    pub fn picture(handle: isize) -> Option<(u32, u32, (u32, u32), Vec<u8>)> {
        unsafe {
            let mut icon = ICONINFO::default();
            GetIconInfo(HICON(handle as *mut _), &mut icon).ok()?;
            let dc = GetDC(None);
            let color = (!icon.hbmColor.is_invalid()).then(|| pixels(dc, icon.hbmColor)).flatten();
            let mask = pixels(dc, icon.hbmMask);
            ReleaseDC(None, dc);
            for bitmap in [icon.hbmColor, icon.hbmMask] {
                if !bitmap.is_invalid() {
                    let _ = DeleteObject(bitmap.into());
                }
            }

            let (width, height, rgba) = match (color, mask) {
                (Some((width, height, mut bgra)), mask) => {
                    // An old-style cursor has colors but no transparency of its own: the mask says what shows.
                    let opaque_by_mask = bgra.chunks_exact(4).all(|pixel| pixel[3] == 0);
                    for (index, pixel) in bgra.chunks_exact_mut(4).enumerate() {
                        pixel.swap(0, 2);
                        if opaque_by_mask {
                            let hidden = mask.as_ref().is_some_and(|(_, _, mask)| mask.get(index * 4).is_some_and(|&bit| bit != 0));
                            pixel[3] = if hidden { 0 } else { 255 };
                        }
                    }
                    (width, height, bgra)
                }
                // Black and white only: the mask's top half says what is see-through, its bottom half
                // what is white, and both together mean "the opposite of what is underneath".
                (None, Some((width, doubled, mask))) => {
                    let height = doubled / 2;
                    let (see_through, white) = mask.split_at((width * height * 4) as usize);
                    let mut rgba = Vec::with_capacity(see_through.len());
                    for (through, white) in see_through.chunks_exact(4).zip(white.chunks_exact(4)) {
                        rgba.extend_from_slice(match (through[0] != 0, white[0] != 0) {
                            (false, false) => &[0, 0, 0, 255],
                            (false, true) => &[255, 255, 255, 255],
                            (true, false) => &[0, 0, 0, 0],
                            // Cannot be done in a picture; a dark gray shows on most backgrounds.
                            (true, true) => &[64, 64, 64, 255],
                        });
                    }
                    (width, height, rgba)
                }
                (None, None) => return None,
            };
            if width > MAX_SIDE || height > MAX_SIDE || height == 0 {
                return None;
            }
            Some((width, height, (icon.xHotspot.min(width - 1), icon.yHotspot.min(height - 1)), rgba))
        }
    }
}

/// macOS has no handle to tell cursors apart by, so each is known by its picture: the handle is a
/// number worked out from the pixels, and the pictures seen are kept to answer for it.
#[cfg(target_os = "macos")]
mod platform {
    use std::{
        collections::HashMap,
        ffi::c_void,
        sync::{Mutex, OnceLock},
        time::{Duration, Instant},
    };

    use objc2::{
        msg_send,
        rc::{autoreleasepool, Retained},
        ClassType,
    };
    use objc2_app_kit::NSCursor;

    use super::MAX_SIDE;
    use crate::{
        mac::{CFRelease, CGPoint, CGRect, CGSize},
        util::lock,
    };

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGColorSpaceCreateDeviceRGB() -> *const c_void;
        fn CGBitmapContextCreate(data: *mut c_void, width: usize, height: usize, bits: usize, row: usize, space: *const c_void, info: u32) -> *const c_void;
        fn CGContextDrawImage(context: *const c_void, area: CGRect, image: *const c_void);
    }

    /// `kCGImageAlphaPremultipliedLast`: red, green, blue, then alpha, with the colors already
    /// multiplied by it.
    const RGBA_PREMULTIPLIED: u32 = 1;
    /// Reading the cursor takes a moment, and ten looks a second are as many as a person notices.
    const LOOK_EVERY: Duration = Duration::from_millis(100);
    /// Pictures remembered before starting over.
    const MAX_KEPT: usize = 64;

    type Picture = (u32, u32, (u32, u32), Vec<u8>);

    #[derive(Default)]
    struct Seen {
        looked: Option<Instant>,
        current: Option<isize>,
        pictures: HashMap<isize, Picture>,
    }

    static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

    /// A cursor's picture at the size it has on screen, in points, which is what a browser sizes its
    /// own cursors in.
    fn read(cursor: &NSCursor) -> Option<Picture> {
        let image = cursor.image();
        let (size, spot) = (image.size(), cursor.hotSpot());
        let (width, height) = (size.width.round() as u32, size.height.round() as u32);
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        unsafe {
            let drawn = image.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None)?;
            let mut rgba = vec![0u8; (width * height * 4) as usize];
            let space = CGColorSpaceCreateDeviceRGB();
            let context = CGBitmapContextCreate(rgba.as_mut_ptr().cast(), width as usize, height as usize, 8, width as usize * 4, space, RGBA_PREMULTIPLIED);
            CFRelease(space);
            if context.is_null() {
                return None;
            }
            let area = CGRect { origin: CGPoint::default(), size: CGSize { width: width as f64, height: height as f64 } };
            CGContextDrawImage(context, area, Retained::as_ptr(&drawn).cast());
            CFRelease(context);

            // A PNG's colors are not multiplied by their transparency.
            for pixel in rgba.chunks_exact_mut(4).filter(|pixel| pixel[3] != 0 && pixel[3] != 255) {
                for color in 0..3 {
                    pixel[color] = (pixel[color] as u32 * 255 / pixel[3] as u32).min(255) as u8;
                }
            }
            let hotspot = ((spot.x.max(0.0) as u32).min(width - 1), (spot.y.max(0.0) as u32).min(height - 1));
            Some((width, height, hotspot, rgba))
        }
    }

    fn number(picture: &Picture) -> isize {
        super::fnv1a(&picture.3, (picture.0, picture.1), picture.2) as isize
    }

    /// The cursor in use on the sharer's screen right now, by the number its picture gives.
    pub fn current() -> Option<isize> {
        let mut seen = lock(&SEEN);
        let seen = seen.get_or_insert_with(Seen::default);
        if seen.looked.is_some_and(|looked| looked.elapsed() < LOOK_EVERY) {
            return seen.current;
        }
        seen.looked = Some(Instant::now());
        // No cursor at all, as while typing in some programs: the helper keeps the one they had.
        let cursor: Option<Retained<NSCursor>> = autoreleasepool(|_| unsafe { msg_send![NSCursor::class(), currentSystemCursor] });
        let picture = autoreleasepool(|_| cursor.and_then(|cursor| read(&cursor)))?;
        let handle = number(&picture);
        if seen.pictures.len() >= MAX_KEPT {
            seen.pictures.clear();
        }
        seen.pictures.insert(handle, picture);
        seen.current = Some(handle);
        seen.current
    }

    /// The name of one of macOS's own cursors, known by having the same picture.
    pub fn name(handle: isize) -> Option<&'static str> {
        static NAMED: OnceLock<Vec<(isize, &'static str)>> = OnceLock::new();
        let named = NAMED.get_or_init(|| {
            autoreleasepool(|_| {
                // Asked for by name: without a window of its own, as when the agent runs headless, AppKit
                // has no cursors to give, and says so with nothing where its headers promise a cursor.
                let own = |cursor: Option<Retained<NSCursor>>, name| cursor.and_then(|cursor| read(&cursor)).map(|picture| (number(&picture), name));
                let class = NSCursor::class();
                unsafe {
                    [
                        own(msg_send![class, arrowCursor], "default"),
                        own(msg_send![class, IBeamCursor], "text"),
                        own(msg_send![class, pointingHandCursor], "pointer"),
                        own(msg_send![class, resizeLeftRightCursor], "ew-resize"),
                        own(msg_send![class, resizeUpDownCursor], "ns-resize"),
                        own(msg_send![class, openHandCursor], "grab"),
                        own(msg_send![class, closedHandCursor], "grabbing"),
                        own(msg_send![class, crosshairCursor], "crosshair"),
                        own(msg_send![class, operationNotAllowedCursor], "not-allowed"),
                        own(msg_send![class, dragCopyCursor], "copy"),
                        own(msg_send![class, dragLinkCursor], "alias"),
                        own(msg_send![class, contextualMenuCursor], "context-menu"),
                    ]
                    .into_iter()
                    .flatten()
                    .collect()
                }
            })
        });
        named.iter().find(|(known, _)| *known == handle).map(|(_, name)| *name)
    }

    /// A drawn cursor's picture: its size, the point that does the pointing, and its pixels as RGBA.
    pub fn picture(handle: isize) -> Option<Picture> {
        lock(&SEEN).as_ref()?.pictures.get(&handle).cloned()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    pub fn current() -> Option<isize> {
        None
    }

    pub fn name(_handle: isize) -> Option<&'static str> {
        None
    }

    pub fn picture(_handle: isize) -> Option<(u32, u32, (u32, u32), Vec<u8>)> {
        let _ = super::MAX_SIDE;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads_as_it_should() {
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(&[b"123456789"]), 0xcbf4_3926);
        // One red pixel: a file any PNG reader accepts starts and ends as it must.
        let file = png(1, 1, &[255, 0, 0, 255]);
        assert_eq!(&file[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(&file[file.len() - 8..file.len() - 4], b"IEND");
    }

    /// macOS's cursors are told apart by their pictures: the arrow has one, and it is not the hand's.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_cursor_in_use_can_be_read() {
        let handle = platform::current().expect("there is a cursor");
        let (width, height, hotspot, rgba) = platform::picture(handle).expect("it has a picture");
        assert_eq!(rgba.len() as u32, width * height * 4);
        assert!(hotspot.0 < width && hotspot.1 < height);
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] == 0) && rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
        eprintln!("the cursor is {width}×{height}, pointing with {hotspot:?}, and is called {:?}", platform::name(handle));
        std::fs::write(std::env::temp_dir().join("light-assist-cursor.png"), png(width, height, &rgba)).unwrap();
    }

    /// Windows' own arrow, read the way a drawn cursor is: it has a shape, and room around it.
    #[cfg(windows)]
    #[test]
    fn a_cursor_can_be_read_as_a_picture() {
        use windows::Win32::UI::WindowsAndMessaging::{LoadCursorW, IDC_ARROW};
        let handle = unsafe { LoadCursorW(None, IDC_ARROW) }.expect("there is an arrow").0 as isize;
        let (width, height, hotspot, rgba) = platform::picture(handle).expect("it has a picture");
        assert_eq!(rgba.len() as u32, width * height * 4);
        assert!(hotspot.0 < width && hotspot.1 < height);
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] == 0) && rgba.chunks_exact(4).any(|pixel| pixel[3] == 255));
        std::fs::write(std::env::temp_dir().join("light-assist-cursor.png"), png(width, height, &rgba)).unwrap();
    }
}

//! The two jobs on a PNG of about 6,000 pixels and 50 MB that each test makes
//! itself. Each job runs in a child process of its own, so the peak memory it
//! reports is that job's alone. Run with `--nocapture` to read the numbers.

use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io::{BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use bae_util::test_files::TestFolder;

use crate::tiles::{cut_tiles, tile_path, Pyramid};
use crate::{make_copy, CopyFormat, IsCancelled, PictureStep, PicturesError};

const WIDTH: u32 = 6000;
const HEIGHT: u32 = 5600;
/// The marker in the stored picture's top-left corner, in pixels.
const MARKER: u32 = 600;
const ORIGINAL: &str = "BAE_PICTURES_ORIGINAL";
const OUTPUT: &str = "BAE_PICTURES_OUTPUT";

/// What the PNG decodes to: every pixel in memory at once, which a streaming
/// job must stay under.
const DECODED_BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * 3;

/// The EXIF block of the PNG's eXIf chunk, which holds one tag: orientation 6,
/// "rotate 90 degrees clockwise to show it upright".
fn exif_orientation_6() -> Vec<u8> {
    let mut exif = Vec::new();
    exif.extend_from_slice(b"MM\x00\x2a\x00\x00\x00\x08");
    exif.extend_from_slice(&1u16.to_be_bytes());
    exif.extend_from_slice(&0x0112u16.to_be_bytes());
    exif.extend_from_slice(&3u16.to_be_bytes());
    exif.extend_from_slice(&1u32.to_be_bytes());
    exif.extend_from_slice(&[0, 6, 0, 0]);
    exif.extend_from_slice(&0u32.to_be_bytes());
    exif
}

/// Writes the test picture row by row, so the test holds one row in memory: a
/// gradient with noise in the low three bits of each channel, which keeps the
/// file near half the size of its pixels, and a solid red square in the
/// top-left corner of the stored picture.
fn write_large_png(folder: &TestFolder, name: &str) {
    let mut info = png::Info::with_size(WIDTH, HEIGHT);
    info.color_type = png::ColorType::Rgb;
    info.bit_depth = png::BitDepth::Eight;
    info.exif_metadata = Some(Cow::Owned(exif_orientation_6()));
    let mut encoder = png::Encoder::with_info(
        BufWriter::new(
            folder.open_with(name, OpenOptions::new().write(true).create(true).truncate(true)),
        ),
        info,
    )
    .unwrap();
    encoder.set_compression(png::Compression::Balanced);
    encoder.set_filter(png::Filter::NoFilter);
    let mut writer = encoder.write_header().unwrap();
    let mut stream = writer.stream_writer().unwrap();
    let mut noise = 0x9e37_79b9_7f4a_7c15u64;
    let mut row = vec![0u8; WIDTH as usize * 3];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            noise ^= noise << 13;
            noise ^= noise >> 7;
            noise ^= noise << 17;
            let at = x as usize * 3;
            if x < MARKER && y < MARKER {
                row[at..at + 3].copy_from_slice(&[255, 0, 0]);
            } else {
                let grain = (noise & 0x07_07_07) as u32;
                row[at] = ((x * 255 / WIDTH) as u8 & 0xf0) | (grain & 0x07) as u8;
                row[at + 1] = ((y * 255 / HEIGHT) as u8 & 0xf0) | ((grain >> 8) & 0x07) as u8;
                row[at + 2] = 0x80 | ((grain >> 16) & 0x07) as u8;
            }
        }
        stream.write_all(&row).unwrap();
    }
    stream.finish().unwrap();
}

/// A job's cost, read from the process that ran it.
struct Cost {
    peak_bytes: u64,
    seconds: f64,
}

/// Runs the ignored test `job` in a child process of this test binary and
/// returns the child's peak resident memory. The child gets the original and
/// the output path through its environment.
fn run_in_child(job: &str, original: &Path, output: &Path) -> Cost {
    let started = Instant::now();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([job, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(ORIGINAL, original)
        .env(OUTPUT, output)
        .spawn()
        .unwrap();
    let peak_bytes = wait_for_peak_bytes(child, job);
    Cost { peak_bytes, seconds: started.elapsed().as_secs_f64() }
}

/// Waits for the child to succeed and reads its peak resident memory from
/// the system, which keeps it for the child's parent.
#[cfg(unix)]
fn wait_for_peak_bytes(child: std::process::Child, job: &str) -> u64 {
    let mut status = 0;
    // SAFETY: an all-zero rusage is a valid value for wait4 to fill in, and
    // the child is this process's own and is waited for only here.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        let pid = libc::pid_t::try_from(child.id()).unwrap();
        assert_eq!(libc::wait4(pid, &mut status, 0, &mut usage), pid);
        usage
    };
    assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0, "{job} failed");
    // macOS counts ru_maxrss in bytes, Linux and Android in kilobytes.
    let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
    u64::try_from(usage.ru_maxrss).unwrap() * unit
}

/// Waits for the child to succeed and reads its peak working set, which
/// Windows keeps for as long as a handle to the process is open.
#[cfg(windows)]
fn wait_for_peak_bytes(mut child: std::process::Child, job: &str) -> u64 {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };

    let status = child.wait().unwrap();
    assert!(status.success(), "{job} failed: {status}");
    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).unwrap();
    counters.cb = size;
    // SAFETY: the child's handle stays open until `child` drops, and
    // `counters` is a valid, sized PROCESS_MEMORY_COUNTERS.
    let read = unsafe { GetProcessMemoryInfo(child.as_raw_handle(), &mut counters, size) };
    assert_ne!(read, 0, "reading the peak memory of {job} failed");
    u64::try_from(counters.PeakWorkingSetSize).unwrap()
}

/// A job that nothing ends.
fn never() -> IsCancelled {
    Arc::new(|| false)
}

fn mebibytes(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn make_original(folder: &TestFolder) -> (PathBuf, u64) {
    write_large_png(folder, "original.png");
    (folder.join("original.png"), folder.metadata("original.png").len())
}

fn child_paths() -> (PathBuf, PathBuf) {
    (
        PathBuf::from(std::env::var_os(ORIGINAL).unwrap()),
        PathBuf::from(std::env::var_os(OUTPUT).unwrap()),
    )
}

#[test]
#[ignore = "runs as the child of a_large_png_makes_an_upright_copy"]
#[expect(
    clippy::disallowed_methods,
    reason = "test code: the child opens the file its parent made in a test folder"
)]
fn copy_job() {
    let (original, output) = child_paths();
    let step = PictureStep::covering(256);
    let copy = make_copy(std::fs::File::open(original).unwrap(), never(), step).unwrap();
    assert_eq!(copy.format, CopyFormat::Jpeg);
    std::fs::write(output, copy.bytes).unwrap();
}

#[test]
fn a_large_png_makes_an_upright_copy() {
    let folder = TestFolder::new();
    let (original, original_bytes) = make_original(&folder);
    let copy_path = folder.join("copy.jpg");
    let cost = run_in_child("tests::copy_job", &original, &copy_path);

    let mut decoder = jpeg_decoder::Decoder::new(folder.open("copy.jpg"));
    let pixels = decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    let (width, height) = (usize::from(info.width), usize::from(info.height));
    println!(
        "copy: {WIDTH}x{HEIGHT} PNG of {:.1} MiB -> {width}x{height} JPEG of {} bytes; peak memory {:.1} MiB, {:.2} s",
        mebibytes(original_bytes),
        folder.metadata("copy.jpg").len(),
        mebibytes(cost.peak_bytes),
        cost.seconds,
    );
    // A request for 256 pixels rounds up to the 384 step. The stored picture
    // is wider than tall and says "rotate clockwise", so the copy is taller
    // than wide and 384 high.
    assert_eq!(height, 384);
    assert!((357..=359).contains(&width), "{width} is not 384 * 5600 / 6000, rounded");
    // The red square, stored top-left, shows top-right once upright.
    let pixel = |x: usize, y: usize| &pixels[(y * width + x) * 3..][..3];
    let [red, green, blue] = <[u8; 3]>::try_from(pixel(width - 8, 8)).unwrap();
    assert!(red > 200 && green < 80 && blue < 80, "top-right pixel is {red} {green} {blue}");
    let [red, green, _] = <[u8; 3]>::try_from(pixel(8, 8)).unwrap();
    assert!(red < 200 || green > 80, "top-left pixel is red");
    assert!(original_bytes > 40 * 1024 * 1024, "the original is {original_bytes} bytes");
    assert!(cost.peak_bytes < DECODED_BYTES, "peak memory held the whole picture");
}

#[test]
#[ignore = "runs as the child of a_large_png_cuts_into_upright_tiles"]
#[expect(
    clippy::disallowed_methods,
    reason = "test code: the child opens the file its parent made in a test folder"
)]
fn tiles_job() {
    let (original, output) = child_paths();
    let pyramid = cut_tiles(std::fs::File::open(original).unwrap(), never(), &output).unwrap();
    assert_eq!(pyramid, Pyramid { width: HEIGHT, height: WIDTH, format: CopyFormat::Jpeg });
}

/// The decoded pixels of one tile, with its width and height.
fn tile_pixels(folder: &TestFolder, path: &Path) -> (Vec<u8>, usize, usize) {
    let relative = path.strip_prefix(folder.path()).unwrap();
    let mut decoder = jpeg_decoder::Decoder::new(folder.open(relative));
    let pixels = decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    (pixels, usize::from(info.width), usize::from(info.height))
}

fn is_red(pixels: &[u8], width: usize, x: usize, y: usize) -> bool {
    let [red, green, blue] = <[u8; 3]>::try_from(&pixels[(y * width + x) * 3..][..3]).unwrap();
    red > 200 && green < 80 && blue < 80
}

#[test]
fn a_large_png_cuts_into_upright_tiles() {
    let folder = TestFolder::new();
    let (original, original_bytes) = make_original(&folder);
    let tiles = folder.join("tiles");
    let cost = run_in_child("tests::tiles_job", &original, &tiles);
    println!(
        "tiles: {WIDTH}x{HEIGHT} PNG of {:.1} MiB; peak memory {:.1} MiB, {:.2} s",
        mebibytes(original_bytes),
        mebibytes(cost.peak_bytes),
        cost.seconds,
    );
    assert!(cost.peak_bytes < DECODED_BYTES, "peak memory held the whole picture");

    // The stored picture is wider than tall and says "rotate clockwise", so
    // the tiles are of a picture taller than wide.
    let pyramid = Pyramid { width: HEIGHT, height: WIDTH, format: CopyFormat::Jpeg };
    for (index, level) in pyramid.levels().iter().enumerate() {
        let index = u32::try_from(index).unwrap();
        let last = tile_path(&tiles, pyramid.format, index, level.columns - 1, level.rows - 1);
        let (_, width, height) = tile_pixels(&folder, &last);
        let edge = |side: u32| ((side - 1) % 512 + 1) as usize;
        assert_eq!((width, height), (edge(level.width), edge(level.height)), "level {index}");
    }
    // The red square, stored top-left, shows top-right once upright: it
    // fills the last column's first tile and reaches into the one before it.
    let columns = pyramid.levels()[0].columns;
    let (pixels, width, _) =
        tile_pixels(&folder, &tile_path(&tiles, pyramid.format, 0, columns - 1, 0));
    assert!(is_red(&pixels, width, 8, 8) && is_red(&pixels, width, width - 8, 8));
    let (pixels, width, _) =
        tile_pixels(&folder, &tile_path(&tiles, pyramid.format, 0, columns - 2, 0));
    assert!(!is_red(&pixels, width, 8, 8) && is_red(&pixels, width, width - 8, 8));
    let (pixels, width, _) = tile_pixels(&folder, &tile_path(&tiles, pyramid.format, 0, 0, 0));
    assert!(!is_red(&pixels, width, 8, 8));
}

#[test]
fn a_picture_with_transparency_cuts_into_png_tiles() {
    let folder = TestFolder::new();
    let original = small_png(700, 300, png::ColorType::Rgba, &[200, 30, 30, 128]);
    let tiles = folder.join("tiles");
    let pyramid = cut_tiles(Cursor::new(original), never(), &tiles).unwrap();
    assert_eq!(pyramid, Pyramid { width: 700, height: 300, format: CopyFormat::Png });
    let levels = pyramid.levels();
    assert_eq!(levels.len(), 2);
    let tile = tile_path(&tiles, CopyFormat::Png, 0, 1, 0);
    let relative = tile.strip_prefix(folder.path()).unwrap();
    let mut bytes = Vec::new();
    folder.open(relative).read_to_end(&mut bytes).unwrap();
    assert_eq!(&bytes[1..4], b"PNG");
}

#[test]
fn a_cancelled_cut_stops_reading_its_original() {
    let original = small_png(2000, 2000, png::ColorType::Rgb, &[10, 120, 200]);
    let cancel = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicUsize::new(0));
    let origin = CancellingOrigin {
        inner: Cursor::new(original),
        reads: reads.clone(),
        cancel: cancel.clone(),
    };
    let watcher = cancel.clone();
    let folder = TestFolder::new();
    let result =
        cut_tiles(origin, Arc::new(move || watcher.load(Ordering::SeqCst)), &folder.join("tiles"));
    assert!(matches!(result, Err(PicturesError::Cancelled)), "{result:?}");
    assert!(reads.load(Ordering::SeqCst) <= 4, "{} reads", reads.load(Ordering::SeqCst));
}

#[test]
fn a_request_rounds_up_to_the_next_step() {
    assert_eq!(PictureStep::covering(1), PictureStep::S192);
    assert_eq!(PictureStep::covering(192), PictureStep::S192);
    assert_eq!(PictureStep::covering(193), PictureStep::S384);
    assert_eq!(PictureStep::covering(768), PictureStep::S768);
    assert_eq!(PictureStep::covering(769), PictureStep::S1536);
    assert_eq!(PictureStep::covering(4000), PictureStep::S1536);
}

/// A PNG of `width` x `height` pixels, each `pixel`.
fn small_png(width: u32, height: u32, color: png::ColorType, pixel: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let data: Vec<u8> = pixel
        .iter()
        .copied()
        .cycle()
        .take(width as usize * height as usize * pixel.len())
        .collect();
    writer.write_image_data(&data).unwrap();
    drop(writer);
    bytes
}

#[test]
fn a_picture_with_transparency_keeps_it_as_a_png() {
    let original = small_png(64, 48, png::ColorType::Rgba, &[200, 30, 30, 128]);
    let copy = make_copy(Cursor::new(original), never(), PictureStep::S192).unwrap();
    assert_eq!(copy.format, CopyFormat::Png);
    assert_eq!(&copy.bytes[1..4], b"PNG");
}

#[test]
fn a_picture_without_transparency_is_a_jpeg_never_larger_than_the_original() {
    let original = small_png(64, 48, png::ColorType::Rgb, &[200, 30, 30]);
    let copy = make_copy(Cursor::new(original), never(), PictureStep::S192).unwrap();
    assert_eq!(copy.format, CopyFormat::Jpeg);
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(copy.bytes));
    decoder.read_info().unwrap();
    let info = decoder.info().unwrap();
    assert_eq!((info.width, info.height), (64, 48));
}

/// A picture that counts how many times it is read and cancels its job after
/// the third read.
struct CancellingOrigin {
    inner: Cursor<Vec<u8>>,
    reads: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

impl Read for CancellingOrigin {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.reads.fetch_add(1, Ordering::SeqCst) + 1 == 3 {
            self.cancel.store(true, Ordering::SeqCst);
        }
        // Small chunks, so the picture takes many reads.
        let end = buffer.len().min(512);
        self.inner.read(&mut buffer[..end])
    }
}

impl Seek for CancellingOrigin {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

/// A picture whose reads fail after `good` bytes, as a network read does.
struct FailingOrigin {
    inner: Cursor<Vec<u8>>,
    good: usize,
}

impl Read for FailingOrigin {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let at = usize::try_from(self.inner.position()).unwrap();
        if at >= self.good {
            return Err(std::io::Error::other("the share went away"));
        }
        let end = buffer.len().min(self.good - at);
        self.inner.read(&mut buffer[..end])
    }
}

impl Seek for FailingOrigin {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

/// A picture that stops being readable fails its cut and its copy with the
/// failed read, whichever format it is in and wherever it stops, even before
/// libvips has read the picture's header, instead of keeping what libvips
/// made of the part it read.
#[test]
fn a_read_that_fails_fails_the_cut_and_the_copy() {
    let photo = {
        // A JPEG, from a copy of a PNG.
        let png = small_png(1500, 1500, png::ColorType::Rgb, &[10, 120, 200]);
        make_copy(Cursor::new(png), never(), PictureStep::S1536).unwrap().bytes
    };
    let png = small_png(1500, 1500, png::ColorType::Rgb, &[10, 120, 200]);
    for (name, original) in [("a JPEG", photo), ("a PNG", png)] {
        for (stops, good) in [("partway", original.len() / 2), ("at once", 0)] {
            let origin = || FailingOrigin { inner: Cursor::new(original.clone()), good };
            let folder = TestFolder::new();
            let cut = cut_tiles(origin(), never(), &folder.join("tiles"));
            assert!(matches!(cut, Err(PicturesError::Read(_))), "{name} {stops}: {cut:?}");
            let copy = make_copy(origin(), never(), PictureStep::S192);
            assert!(matches!(copy, Err(PicturesError::Read(_))), "{name} {stops}: {copy:?}");
        }
    }
}

#[test]
fn a_cancelled_copy_stops_reading_its_original() {
    let original = small_png(2000, 2000, png::ColorType::Rgb, &[10, 120, 200]);
    let cancel = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicUsize::new(0));
    let origin = CancellingOrigin {
        inner: Cursor::new(original),
        reads: reads.clone(),
        cancel: cancel.clone(),
    };
    let watcher = cancel.clone();
    let result =
        make_copy(origin, Arc::new(move || watcher.load(Ordering::SeqCst)), PictureStep::S192);
    assert!(matches!(result, Err(PicturesError::Cancelled)), "{result:?}");
    // The read that cancelled was the third. libvips may already have asked
    // for the next read, and no read starts once the flag is set.
    assert!(reads.load(Ordering::SeqCst) <= 4, "{} reads", reads.load(Ordering::SeqCst));
}

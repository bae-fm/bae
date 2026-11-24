//! The two jobs on a PNG of about 6,000 pixels and 50 MB that each test makes
//! itself. Each job runs in a child process of its own, so the peak memory it
//! reports is that job's alone. Run with `--nocapture` to read the numbers.

use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use bae_util::test_files::TestFolder;

use crate::{cut_tiles, thumbnail_jpeg};

const WIDTH: u32 = 6000;
const HEIGHT: u32 = 5600;
/// The marker in the stored picture's top-left corner, in pixels.
const MARKER: u32 = 600;
const THUMBNAIL_EDGE: u32 = 256;
const TILE_EDGE: u32 = 512;
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
    #[expect(
        clippy::zombie_processes,
        reason = "wait4 below reaps the child, which is how its resource usage is read"
    )]
    let child = Command::new(std::env::current_exe().unwrap())
        .args([job, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(ORIGINAL, original)
        .env(OUTPUT, output)
        .spawn()
        .unwrap();
    let mut status = 0;
    // SAFETY: an all-zero rusage is a valid value for wait4 to fill in, and
    // the child is this process's own and is waited for only here.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        let pid = libc::pid_t::try_from(child.id()).unwrap();
        assert_eq!(libc::wait4(pid, &mut status, 0, &mut usage), pid);
        usage
    };
    let seconds = started.elapsed().as_secs_f64();
    assert!(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0, "{job} failed");
    // macOS counts ru_maxrss in bytes, Linux and Android in kilobytes.
    let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
    Cost { peak_bytes: u64::try_from(usage.ru_maxrss).unwrap() * unit, seconds }
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
#[ignore = "runs as the child of thumbnail_of_a_large_png"]
fn thumbnail_job() {
    let (original, output) = child_paths();
    thumbnail_jpeg(&original, &output, THUMBNAIL_EDGE).unwrap();
}

#[test]
#[ignore = "runs as the child of tiles_of_a_large_png"]
fn tiles_job() {
    let (original, output) = child_paths();
    cut_tiles(&original, &output, TILE_EDGE).unwrap();
}

#[test]
fn thumbnail_of_a_large_png() {
    let folder = TestFolder::new();
    let (original, original_bytes) = make_original(&folder);
    let thumbnail = folder.join("thumbnail.jpg");
    let cost = run_in_child("tests::thumbnail_job", &original, &thumbnail);

    let mut decoder = jpeg_decoder::Decoder::new(folder.open("thumbnail.jpg"));
    let pixels = decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    let (width, height) = (usize::from(info.width), usize::from(info.height));
    println!(
        "thumbnail: {WIDTH}x{HEIGHT} PNG of {:.1} MiB -> {width}x{height} JPEG of {} bytes; peak memory {:.1} MiB, {:.2} s",
        mebibytes(original_bytes),
        folder.metadata("thumbnail.jpg").len(),
        mebibytes(cost.peak_bytes),
        cost.seconds,
    );
    // The stored picture is wider than tall and says "rotate clockwise", so
    // the thumbnail is taller than wide and 256 high.
    assert_eq!(height, THUMBNAIL_EDGE as usize);
    assert!((238..=240).contains(&width), "{width} is not 256 * 5600 / 6000, rounded");
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
fn tiles_of_a_large_png() {
    let folder = TestFolder::new();
    let (original, original_bytes) = make_original(&folder);
    let tiles = folder.join("tiles");
    let cost = run_in_child("tests::tiles_job", &original, &tiles);

    let mut count = 0usize;
    let mut tile_bytes = 0u64;
    let mut level_count = 0usize;
    // libvips also writes a properties file beside the level folders.
    for level in
        folder.entries("tiles_files").into_iter().filter(|name| name.parse::<u32>().is_ok())
    {
        level_count += 1;
        for tile in folder.entries(format!("tiles_files/{level}")) {
            let path = format!("tiles_files/{level}/{tile}");
            let mut decoder = jpeg_decoder::Decoder::new(folder.open(&path));
            decoder.read_info().unwrap();
            let info = decoder.info().unwrap();
            assert!(
                u32::from(info.width) <= TILE_EDGE && u32::from(info.height) <= TILE_EDGE,
                "{path}"
            );
            count += 1;
            tile_bytes += folder.metadata(&path).len();
        }
    }
    println!(
        "tiles: {WIDTH}x{HEIGHT} PNG of {:.1} MiB -> {count} tiles in {level_count} levels, {:.1} MiB in all ({:.0}% of the original); peak memory {:.1} MiB, {:.2} s",
        mebibytes(original_bytes),
        mebibytes(tile_bytes),
        tile_bytes as f64 * 100.0 / original_bytes as f64,
        mebibytes(cost.peak_bytes),
        cost.seconds,
    );
    // 6000 x 5600 at 512: 12 x 11 tiles on the full-size level, and a level
    // for every halving down to one pixel (levels 0 to 13).
    assert_eq!(level_count, 14);
    assert!(count >= 12 * 11, "{count} tiles");
    assert!(folder.exists("tiles.dzi"));
    assert!(cost.peak_bytes < DECODED_BYTES, "peak memory held the whole picture");
}

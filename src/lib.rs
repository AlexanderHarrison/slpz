//! Compresses and decompresses between the slp and slpz formats.
//!
//! You can expect slpz files to be around 8x to 12x times smaller than slp files for regular matches.
//! (~3Mb down to ~300Kb).
//!
//! Compression is done with the zstd compression library.
//! zstd is not required on the user's computer; the library is statically linked at compile time.
//!
//! The slpz format is documented in the readme in the repo.
//! Important information, such as player tags, stages, date, characters, etc. all remain uncompressed in the slpz format.
//! This allows slp file browsers to easily parse and display this information without needing to decompress the replay.

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum CompError {
    InvalidFile,
    CompressionFailure,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DecompError {
    InvalidFile,
    DecompressionFailure,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TargetPathError {
    PathNotFound,
    PathInvalid,
    CompressOrDecompressAmbiguous,
    ZstdInitError,
}

impl std::fmt::Display for CompError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            CompError::InvalidFile => "File is invalid",
            CompError::CompressionFailure => "Compression failed",
        })
    }
}

impl std::fmt::Display for DecompError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            DecompError::InvalidFile => "File is invalid",
            DecompError::DecompressionFailure => "Decompression failed",
        })
    }
}

impl std::fmt::Display for TargetPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            TargetPathError::PathNotFound => "Replay path not found",
            TargetPathError::PathInvalid => "Replay path invalid",
            TargetPathError::CompressOrDecompressAmbiguous => "Not a slp or slpz file",
            TargetPathError::ZstdInitError => "Failed to init zstd",
        })
    }
}

const EVENT_PAYLOADS: u8 = 0x35;
const GAME_START: u8 = 0x36;
const RAW_HEADER: [u8; 11] = [0x7B, 0x55, 0x03, 0x72, 0x61, 0x77, 0x5B, 0x24, 0x55, 0x23, 0x6C];

pub const VERSION: u32 = 0;

pub struct Compressor { ctx: zstd::bulk::Compressor<'static> }
pub struct Decompressor { ctx: zstd::bulk::Decompressor<'static> }

impl Compressor {
    /// compression_level should be between 1..=19. The default is 3.
    pub fn new(compression_level: i32) -> Option<Compressor> {
        Some(Compressor {
            ctx: zstd::bulk::Compressor::new(compression_level).ok()?,
        })
    }
}

impl Decompressor {
    pub fn new() -> Option<Decompressor> {
        Some(Decompressor { ctx: zstd::bulk::Decompressor::new().ok()? })
    }
}

/// Compresses an slp file to an slpz file.
pub fn compress(compressor: &mut Compressor, slp: &[u8]) -> Result<Vec<u8>, CompError> {
    if slp.len() < 16 { return Err(CompError::InvalidFile) }
    if slp[0..11] != RAW_HEADER { return Err(CompError::InvalidFile) }

    // get metadata
    let raw_len = u32::from_be_bytes(slp[11..15].try_into().unwrap()) as usize;
    let metadata_offset = 15 + raw_len;
    if slp.len() < metadata_offset as usize { return Err(CompError::InvalidFile) }
    let metadata = &slp[metadata_offset..];

    // get event sizes
    if slp[15] != EVENT_PAYLOADS { return Err(CompError::InvalidFile) }
    let (event_sizes, event_type_count) = event_sizes(&slp[15..]).ok_or(CompError::InvalidFile)?;
    let event_sizes_size = 2 + event_type_count * 3;
    let event_sizes_payload = &slp[15..][..event_sizes_size];

    // get game start
    let game_start_offset = 15 + event_sizes_size;
    let game_start_size = event_sizes[GAME_START as usize] as usize + 1;
    if slp.len() < game_start_offset+game_start_size { return Err(CompError::InvalidFile) }
    if slp[game_start_offset] != GAME_START { return Err(CompError::InvalidFile) }
    let game_start_payload = &slp[game_start_offset..][..game_start_size];

    let other_events_offset = game_start_offset+game_start_size;
    if metadata_offset < other_events_offset { return Err(CompError::InvalidFile); }

    let mut slpz = Vec::with_capacity(slp.len());

    // header
    slpz.extend_from_slice(&VERSION.to_be_bytes());
    slpz.extend_from_slice(&[0u8; 20]); // offsets filled later

    // write event sizes
    let len = slpz.len() as u32;
    slpz[4..8].copy_from_slice(&len.to_be_bytes());
    slpz.extend_from_slice(event_sizes_payload);

    // write game start
    let len = slpz.len() as u32;
    slpz[8..12].copy_from_slice(&len.to_be_bytes());
    slpz.extend_from_slice(game_start_payload);

    // write metadata
    let len = slpz.len() as u32;
    slpz[12..16].copy_from_slice(&len.to_be_bytes());
    slpz.extend_from_slice(metadata);

    // write compressed events
    let len = slpz.len() as u32;
    slpz[16..20].copy_from_slice(&len.to_be_bytes());

    let mut reordered_data = Vec::with_capacity(slp.len());
    let written = reorder_events(&slp[other_events_offset..metadata_offset], &event_sizes, &mut reordered_data)?;
    slpz[20..24].copy_from_slice(&(written as u32).to_be_bytes());

    // wrap in cursor so we don't overwrite previous data
    let mut slpz_cursor = std::io::Cursor::new(slpz);
    slpz_cursor.set_position(len as u64);
    compressor.ctx.compress_to_buffer(&reordered_data, &mut slpz_cursor).map_err(|_| CompError::CompressionFailure)?;

    Ok(slpz_cursor.into_inner())
}

/// Decompresses an slpz file to an slp file.
#[rustfmt::skip]
pub fn decompress(decompressor: &mut Decompressor, slpz: &[u8]) -> Result<Vec<u8>, DecompError> {
    if slpz.len() < 24 { return Err(DecompError::InvalidFile) }
    let version                  = u32::from_be_bytes(slpz[0..4].try_into().unwrap());
    let event_sizes_offset       = u32::from_be_bytes(slpz[4..8].try_into().unwrap()) as usize;
    let game_start_offset        = u32::from_be_bytes(slpz[8..12].try_into().unwrap()) as usize;
    let metadata_offset          = u32::from_be_bytes(slpz[12..16].try_into().unwrap()) as usize;
    let compressed_events_offset = u32::from_be_bytes(slpz[16..20].try_into().unwrap()) as usize;
    let decompressed_events_size = u32::from_be_bytes(slpz[20..24].try_into().unwrap()) as usize;

    if slpz.len() < compressed_events_offset { return Err(DecompError::InvalidFile) }

    // We do not return a custom version error here.
    // If a file is invalid, it would raise this error instead of an InvalidFile.
    // Unsupported version errors would be nice to check, but too many false positives.
    if version > VERSION { return Err(DecompError::InvalidFile) }

    let mut slp = Vec::with_capacity(slpz.len() * 32);
    slp.extend_from_slice(&RAW_HEADER);
    slp.extend_from_slice(&[0u8; 4]); // raw len. filled in later

    let event_sizes_bytes = &slpz[event_sizes_offset..game_start_offset];
    slp.extend_from_slice(event_sizes_bytes);
    let (event_sizes, _) = event_sizes(event_sizes_bytes).ok_or(DecompError::InvalidFile)?;
    slp.extend_from_slice(&slpz[game_start_offset..metadata_offset]);

    let b = decompressor.ctx.decompress(&slpz[compressed_events_offset..], decompressed_events_size)
        .map_err(|_| DecompError::DecompressionFailure)?;
    unorder_events(&b, &event_sizes, &mut slp)?;

    let metadata_offset_in_slp = slp.len();
    slp.extend_from_slice(&slpz[metadata_offset..compressed_events_offset]);

    slp[11..15].copy_from_slice(&(metadata_offset_in_slp as u32 - 15).to_be_bytes()); // raw len

    Ok(slp)
}

/// Reorders events into byte columns.
fn reorder_events(
    events: &[u8],
    event_sizes: &[u16; 256],
    buf: &mut Vec<u8>,
) -> Result<usize, CompError> {
    let event_counts = event_counts(events, event_sizes)?;

    // ---------------------------------------
    // Build the offset lookup table 'reordered_event_offsets'.
    // This is the offset of the start of the reordered data for each event in the reordered event data section.

    let mut total_events = 0usize;
    let mut reordered_event_offsets = [0u32; 256];

    for i in 0..255 {
        let size = event_sizes[i];
        let count = event_counts[i];
        total_events += count as usize;

        let event_total_size = size as u32 * count;

        // offset for next event is the end of this event.
        reordered_event_offsets[i + 1] = reordered_event_offsets[i] + event_total_size;
    }

    let reordered_size = {
        let last_size = event_sizes[255];
        let last_count = event_counts[255];
        total_events += last_count as usize;
        let last_total_size = last_count as usize * last_size as usize;

        reordered_event_offsets[255] as usize + last_total_size
    };

    if reordered_size != events.len() - total_events { return Err(CompError::InvalidFile) }

    // alloc
    let data_size = 4 + total_events + reordered_size;
    let buf_prev = buf.len();
    buf.resize(buf_prev + data_size, 0u8);
    let data = &mut buf[buf_prev..];

    // ---------------------------------------
    // fill event order list and reordered data

    data[0..4].copy_from_slice(&(total_events as u32).to_be_bytes());

    let event_order_list_offset = 4;
    let reordered_events_offset = event_order_list_offset + total_events;

    let mut events_written = [0u32; 256];
    let mut event_i = 0;
    let mut i = 0;
    while i < events.len() {
        let event_u8 = events[i];
        let event = event_u8 as usize;

        // fill event order list
        data[event_order_list_offset + event_i] = event_u8;

        // fill reorder data
        let event_offset = reordered_events_offset + reordered_event_offsets[event] as usize;
        let written = events_written[event] as usize;
        let size = event_sizes[event] as usize;
        let stride = event_counts[event] as usize;

        let write_start = event_offset + written;
        for j in 0..size {
            data[write_start + j * stride] = events[1 + i + j];
        }

        events_written[event] += 1;

        i += 1 + size;
        event_i += 1;
    }

    Ok(data_size)
}

/// Undoes the reordering done by 'reorder_events'.
///
/// Returns the number of bytes written.
pub(crate) fn unorder_events(
    b: &[u8],
    event_sizes: &[u16; 256],
    buf: &mut Vec<u8>,
) -> Result<usize, DecompError> {
    let total_events = u32::from_be_bytes(b[0..4].try_into().unwrap()) as usize;

    let event_order_list_offset = 4;
    let reordered_events_offset = event_order_list_offset + total_events;

    let mut event_counts = [0u32; 256];
    for i in 0..total_events {
        let event = b[event_order_list_offset + i] as usize;
        event_counts[event] += 1;
    }

    let mut reordered_event_offsets = [0u32; 256];
    for i in 0..255 {
        let size = event_sizes[i];
        let count = event_counts[i];

        let event_total_size = size as u32 * count;

        // offset for next event is the end of this event.
        reordered_event_offsets[i + 1] = reordered_event_offsets[i] + event_total_size;
    }

    let unordered_size = {
        let last_size = event_sizes[255];
        let last_count = event_counts[255];
        let last_total_size = last_count as usize * last_size as usize;
        reordered_event_offsets[255] as usize + last_total_size + total_events
    };

    let event_order_list = &b[event_order_list_offset..reordered_events_offset];
    let events = &b[reordered_events_offset..];

    if unordered_size != events.len() + total_events { return Err(DecompError::InvalidFile) }

    let buf_prev = buf.len();
    buf.resize(buf_prev + unordered_size, 0u8);
    let data = &mut buf[buf_prev..];

    let mut events_written = [0u32; 256];

    let mut data_i = 0;
    for &event_u8 in event_order_list.iter().take(total_events) {
        let event = event_u8 as usize;

        // command byte
        data[data_i] = event_u8;

        // unorder data
        let event_offset = reordered_event_offsets[event] as usize;
        let written = events_written[event] as usize;
        let size = event_sizes[event] as usize;
        let stride = event_counts[event] as usize;

        let write_start = event_offset + written;
        for j in 0..size {
            data[1 + data_i + j] = events[write_start + j * stride];
        }

        events_written[event] += 1;

        data_i += 1 + size;
    }

    Ok(unordered_size)
}

fn event_sizes(events: &[u8]) -> Option<([u16; 256], usize)> {
    if events.is_empty() { return None }

    let info_size = events[1] as usize;
    let event_count = (info_size - 1) / 3;

    if events.len() < info_size { return None }

    let mut event_payload_sizes = [0; 256];
    for i in 0..event_count {
        let offset = i * 3 + 2;
        let command_byte = events[offset] as usize;
        let payload_size = u16::from_be_bytes(events[offset + 1..][..2].try_into().unwrap());
        event_payload_sizes[command_byte] = payload_size;
    }

    Some((event_payload_sizes, event_count))
}

fn event_counts(events: &[u8], event_sizes: &[u16; 256]) -> Result<[u32; 256], CompError> {
    let mut i = 0;
    let mut counts = [0u32; 256];

    while i < events.len() {
        let event = events[i] as usize;
        let event_size = event_sizes[event];
        if event_size == 0 { return Err(CompError::InvalidFile) }
        counts[event] += 1;
        i += 1 + event_size as usize; // skip command byte and payload
    }

    Ok(counts)
}

#[derive(Copy, Clone, Debug)]
pub struct Options {
    pub keep: bool,
    pub compress: Option<bool>,
    pub recursive: bool,
    pub threading: bool,
    /// must be between 1 and 19.
    pub level: i32,
    pub log: bool,
}

impl Default for Options {
    fn default() -> Self { Options::DEFAULT }
}

impl Options {
    pub const DEFAULT: Self = Options {
        keep: true,
        compress: None,
        recursive: false,
        threading: true,
        level: 3,
        log: true,
    };
}

/// Library access to slpz program functionality.
///
/// If Some, the sender will first send the number of targets.
/// After that, the sender will send '1' for each target completed.
/// If the sender cannot send, it will panic.
///
/// - Threaded directory compression/decompression.
/// - Compression/decompression autodetection.
/// - Deletion of old files.
pub fn target_path(
    options: &Options,
    path: &std::path::Path,
    sender: Option<std::sync::mpsc::Sender<usize>>,
) -> Result<(), TargetPathError> {
    if !matches!(path.try_exists(), Ok(true)) { return Err(TargetPathError::PathNotFound) }

    let mut targets = Vec::new();
    let mut should_compress = options.compress;

    if path.is_dir() {
        let c = match should_compress {
            Some(c) => c,
            None => return Err(TargetPathError::CompressOrDecompressAmbiguous),
        };
        let ex = std::ffi::OsStr::new(if c { "slp" } else { "slpz" });
        get_targets(&mut targets, path, options.recursive, ex);
    } else if path.is_file() {
        targets.push(path.to_path_buf());
        if should_compress.is_none() {
            let ex = path.extension();
            if ex == Some(std::ffi::OsStr::new("slp")) {
                should_compress = Some(true);
            } else if ex == Some(std::ffi::OsStr::new("slpz")) {
                should_compress = Some(false);
            }
        }
    } else {
        return Err(TargetPathError::PathInvalid);
    }

    let will_compress = match should_compress {
        Some(n) => n,
        None => return Err(TargetPathError::CompressOrDecompressAmbiguous),
    };

    if let Some(ref sender) = sender {
        sender.send(targets.len()).expect("Sending failed");
    }

    if !options.threading || targets.len() < 8 {
        if will_compress {
            let mut compressor = Compressor::new(options.level).ok_or(TargetPathError::ZstdInitError)?;
            for t in &targets {
                compress_target(&mut compressor, options, t);
                if let Some(ref sender) = sender { sender.send(1).expect("Sending failed"); }
            }
        } else {
            let mut decompressor = Decompressor::new().ok_or(TargetPathError::ZstdInitError)?;
            for t in &targets {
                decompress_target(&mut decompressor, options, t);
                if let Some(ref sender) = sender { sender.send(1).expect("Sending failed"); }
            }
        }
    } else {
        // split into 8 approximately equal slices (why is this so annoying?)
        let mut slices: [&[std::path::PathBuf]; 8] = [&[]; 8];
        let chunk = targets.len() / 8;
        let split = (chunk + 1) * (targets.len() % 8);
        for (i, c) in targets[..split].chunks(chunk+1).chain(targets[split..].chunks(chunk)).enumerate() {
            slices[i] = c;
        }

        let sender_ref = sender.as_ref();

        std::thread::scope(|scope| {
            if will_compress {
                for s in slices {
                    scope.spawn(move || {
                        let sender = sender_ref;
                        let mut compressor = match Compressor::new(options.level) {
                            Some(c) => c,
                            None => {
                                eprintln!("Error: Failed to init zstd compressor");
                                return;
                            }
                        };
                        for t in s {
                            compress_target(&mut compressor, options, t);
                            if let Some(sender) = sender { sender.send(1).expect("Sending failed"); }
                        }
                    });
                }
            } else {
                for s in slices {
                    scope.spawn(move || {
                        let sender = sender_ref;
                        let mut decompressor = match Decompressor::new() {
                            Some(d) => d,
                            None => {
                                eprintln!("Error: Failed to init zstd decompressor");
                                return;
                            }
                        };
                        for t in s {
                            decompress_target(&mut decompressor, options, t);
                            if let Some(sender) = sender { sender.send(1).expect("Sending failed"); }
                        }
                    });
                }
            };
        })
    }

    Ok(())
}

fn compress_target(c: &mut Compressor, options: &Options, t: &std::path::PathBuf) {
    let slp = match std::fs::read(t) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error compressing {}: {}", t.display(), e);
            return;
        }
    };

    match compress(c, &slp) {
        Ok(slpz) => {
            let mut out = t.clone();
            if !out.set_extension("slpz") {
                eprintln!("Error creating new filename for {}", t.display());
                return;
            };
            match std::fs::write(&out, &slpz) {
                Ok(_) => {
                    if options.log { println!("compressed {}", t.display()); }
                    if !options.keep {
                        match std::fs::remove_file(t) {
                            Ok(_) => if options.log { println!("removed {}", t.display()) },
                            Err(e) => {
                                eprintln!("Error removing {}: {}", t.display(), e);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error compressing {}: {}", t.display(), e);
                }
            }
        }
        Err(e) => {
            eprintln!("Error compressing {}: {}", t.display(), e);
        }
    }
}

fn decompress_target(d: &mut Decompressor, options: &Options, t: &std::path::PathBuf) {
    let slpz = match std::fs::read(t) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error decompressing {}: {}", t.display(), e);
            return;
        }
    };

    match decompress(d, &slpz) {
        Ok(slp) => {
            let mut out = t.clone();
            if !out.set_extension("slp") {
                eprintln!("Error creating new filename for {}", t.display());
                return;
            };
            match std::fs::write(&out, &slp) {
                Ok(_) => {
                    if options.log { println!("decompressed {}", t.display()); }
                    if !options.keep {
                        match std::fs::remove_file(t) {
                            Ok(_) => if options.log { println!("removed {}", t.display()) },
                            Err(e) => {
                                eprintln!("Error removing {}: {}", t.display(), e);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error decompressing {}: {}", t.display(), e);
                }
            }
        }
        Err(e) => {
            eprintln!("Error decompressing {}: {}", t.display(), e);
        }
    }
}

fn get_targets(
    targets: &mut Vec<std::path::PathBuf>,
    path: &std::path::Path,
    rec: bool,
    ex: &std::ffi::OsStr,
) -> Option<()> {
    for f in std::fs::read_dir(path).ok()? {
        let f = match f {
            Ok(f) => f,
            Err(_) => continue,
        };

        let path = f.path();

        if rec && path.is_dir() { get_targets(targets, &path, rec, ex); }
        if path.is_file() && path.extension() == Some(ex) { targets.push(path)}
    }

    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorder_round_trip() {
        #[rustfmt::skip]
        let events = [
            3, 1, 2, 3, 4, 5,
            1, 0, 1, 2,
            1, 10, 11, 12,
            2, 1,
            2, 2,
            3, 1, 2, 3, 4, 5,
            1, 20, 21, 22
        ];
        let mut event_sizes = [0u16; 256];
        event_sizes[..4].copy_from_slice(&[0, 3, 1, 5]);

        let mut reordered = Vec::new();
        reorder_events(&events, &event_sizes, &mut reordered).unwrap();
        println!("{:?}", reordered);

        let mut unordered = Vec::new();
        unorder_events(&reordered, &event_sizes, &mut unordered).unwrap();

        assert_eq!(events.as_slice(), &unordered);
    }
}

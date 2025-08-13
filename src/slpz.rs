use clap::Parser;
use slpz::*;
use std::io::{Read, Write};

#[derive(Parser)]
#[command(
    name = "slpz",
    version,
    about = "Compresses and decompresses between the slp and slpz Slippi replay formats.",
    long_about = None,
    after_help = "Examples:
  slpz replay.slp                      # Compress to replay.slpz
  slpz -o output.slpz input.slp        # Compress to specified output
  slpz -x - < input.slp > output.slpz  # Compress from stdin to stdout
  cat replay.slp | slpz -x -o - > compressed.slpz  # Pipe compression"
)]
struct Args {
    /// Input file path (use '-' for stdin)
    input: String,

    /// Output file path (use '-' for stdout)
    #[arg(short = 'o', long = "output")]
    output: Option<String>,

    /// Compress the input
    #[arg(short = 'x', long = "compress", conflicts_with = "decompress")]
    compress: bool,

    /// Decompress the input
    #[arg(short = 'd', long = "decompress")]
    decompress: bool,

    /// Prefer speed over compression
    #[arg(long = "fast", conflicts_with = "small")]
    fast: bool,

    /// Prefer compression over speed
    #[arg(long = "small")]
    small: bool,

    /// Compress/decompress all files in subdirectories
    #[arg(short = 'r', long = "recursive")]
    recursive: bool,

    /// Keep files after compression/decompression
    #[arg(short = 'k', long = "keep", conflicts_with = "remove")]
    keep: bool,

    /// Remove files after compression/decompression
    #[arg(long = "rm")]
    remove: bool,

    /// Do not log to stdout
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

fn main() {
    let args = Args::parse();

    // Build options from args
    let mut options = Options::DEFAULT;
    options.log = !args.quiet;
    options.recursive = args.recursive;

    if args.fast {
        options.level = 3;
    } else if args.small {
        options.level = 12;
    }

    if args.remove {
        options.keep = false;
    } else if args.keep {
        options.keep = true;
    }

    if args.compress {
        options.compress = Some(true);
    } else if args.decompress {
        options.compress = Some(false);
    }

    // If using stdin/stdout with directories, that's an error
    if (args.input == "-" || args.output.as_deref() == Some("-")) && options.recursive {
        eprintln!("Error: cannot use stdin/stdout with recursive directory processing");
        std::process::exit(1);
    }

    // Handle directory processing (original behavior)
    if args.input != "-" && std::path::Path::new(&args.input).is_dir() {
        if args.output.is_some() {
            eprintln!("Error: cannot specify output path when processing directories");
            std::process::exit(1);
        }
        if let Err(e) = target_path(&options, std::path::Path::new(&args.input), None) {
            match e {
                TargetPathError::PathNotFound => eprintln!("Error: input path '{}' not found", &args.input),
                TargetPathError::PathInvalid => eprintln!("Error: input path '{}' not valid", &args.input),
                TargetPathError::CompressOrDecompressAmbiguous => eprintln!("Error: must pass either '-x' or '-d' flag for input path '{}'", &args.input),
                TargetPathError::ZstdInitError => eprintln!("Error: zstd initiation failed"),
            }
        }
        return;
    }

    // Read input
    let input_data = if args.input == "-" {
        let mut buf = Vec::new();
        if let Err(e) = std::io::stdin().read_to_end(&mut buf) {
            eprintln!("Error reading from stdin: {}", e);
            std::process::exit(1);
        }
        buf
    } else {
        match std::fs::read(&args.input) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error reading {}: {}", args.input, e);
                std::process::exit(1);
            }
        }
    };

    // Determine compress/decompress
    let will_compress = match options.compress {
        Some(c) => c,
        None => {
            if args.input == "-" {
                eprintln!("Error: must specify -x/--compress or -d/--decompress when using stdin");
                std::process::exit(1);
            }
            let path = std::path::Path::new(&args.input);
            if path.extension() == Some(std::ffi::OsStr::new("slp")) {
                true
            } else if path.extension() == Some(std::ffi::OsStr::new("slpz")) {
                false
            } else {
                eprintln!("Error: cannot determine whether to compress or decompress {}", args.input);
                eprintln!("Use -x/--compress or -d/--decompress");
                std::process::exit(1);
            }
        }
    };

    // Process data
    let output_data = if will_compress {
        let mut compressor = match Compressor::new(options.level) {
            Some(c) => c,
            None => {
                eprintln!("Error: Failed to init zstd compressor");
                std::process::exit(1);
            }
        };
        match compress(&mut compressor, &input_data) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error compressing: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        let mut decompressor = match Decompressor::new() {
            Some(d) => d,
            None => {
                eprintln!("Error: Failed to init zstd decompressor");
                std::process::exit(1);
            }
        };
        match decompress(&mut decompressor, &input_data) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Error decompressing: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Write output
    match args.output {
        Some(path) if path == "-" => {
            // Write to stdout
            if let Err(e) = std::io::stdout().write_all(&output_data) {
                eprintln!("Error writing to stdout: {}", e);
                std::process::exit(1);
            }
        }
        Some(path) => {
            // Write to specified file
            if let Err(e) = std::fs::write(&path, &output_data) {
                eprintln!("Error writing to {}: {}", path, e);
                std::process::exit(1);
            }
            if options.log {
                println!("{} {} to {}",
                    if will_compress { "compressed" } else { "decompressed" },
                    if args.input == "-" { "stdin" } else { &args.input },
                    path);
            }
        }
        None => {
            // Auto-generate output filename (original behavior for files)
            if args.input == "-" {
                // If no output specified and input is stdin, write to stdout
                if let Err(e) = std::io::stdout().write_all(&output_data) {
                    eprintln!("Error writing to stdout: {}", e);
                    std::process::exit(1);
                }
            } else {
                let mut out_path = std::path::PathBuf::from(&args.input);
                let success = if will_compress {
                    out_path.set_extension("slpz")
                } else {
                    out_path.set_extension("slp")
                };
                if !success {
                    eprintln!("Error creating output filename for {}", args.input);
                    std::process::exit(1);
                }
                if let Err(e) = std::fs::write(&out_path, &output_data) {
                    eprintln!("Error writing to {}: {}", out_path.display(), e);
                    std::process::exit(1);
                }
                if options.log {
                    println!("{} {}",
                        if will_compress { "compressed" } else { "decompressed" },
                        args.input);
                }
                // Handle file removal if --rm was specified
                if !options.keep {
                    if let Err(e) = std::fs::remove_file(&args.input) {
                        eprintln!("Error removing {}: {}", args.input, e);
                    } else if options.log {
                        println!("removed {}", args.input);
                    }
                }
            }
        }
    }
}
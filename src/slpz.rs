use slpz::*;

const HELP: &str =
"Usage: slpz [OPTIONS] <input path>

Options:
  --fast                Prefer speed over compression [Default]
  --small               Prefer compression over speed
  -x, --compress
  -d, --decompress
  -r, --recursive       Compress/decompress all files in subdirectories.
  -k, --keep            Keep files after compression/decompression. [Default]
  --rm                  Remove files after compression/decompression.
  -q, --quiet           Do not log to stdout.
  -o, --output <file>   Specify output file. Pass '-' for stdout. Pass '-' as the input path for stdin.
  -h, --help
  -v, --version";

fn main() {
    let mut options = Options::DEFAULT;

    let mut arg_strings = std::env::args();
    arg_strings.next(); // skip exe name
    let mut arg_strings = arg_strings.collect::<Vec<_>>();

    // last arg is path
    let input_path = match arg_strings.pop() {
        Some(p) => p,
        None => {
            eprintln!("{}", HELP);
            std::process::exit(1);
        }
    };

    if &input_path == "-h" || &input_path == "--help" {
        println!("{}", HELP);
        std::process::exit(0);
    }

    if &input_path == "-v" || &input_path == "--version" {
        println!("slpz version {} - created by Alex Harrison (Aitch)", VERSION);
        std::process::exit(0);
    }

    let mut i = 0;
    while let Some(a) = arg_strings.get(i) {
        match a.as_ref() {
            "--fast" => options.level = 3,
            "--small" => options.level = 12,
            "-x" | "--compress" => options.compress = Some(true),
            "-d" | "--decompress" => options.compress = Some(false),
            "-r" | "--recursive" => options.recursive = true,
            "-k" | "--keep" => options.keep = true,
            "--rm" => options.keep = false,
            "-q" | "--quiet" => options.log = false,
            "-o" | "--output" => {
                i += 1;
                let Some(out_path) = arg_strings.get(i) else {
                    eprintln!("Error: arg '{}' requires an argument!", &a);
                    std::process::exit(1);
                };
                options.output_path = Some(out_path.clone().into());
            }
            "-h" | "--help" => {
                println!("{}", HELP);
                std::process::exit(0);
            }
            "-v" | "--version" => {
                println!("slpz version {} - created by Alex Harrison (Aitch)", VERSION);
                std::process::exit(0);
            }
            a => eprintln!("unknown argument '{}'", a),
        }

        i += 1;
    }
    
    if &input_path == "-" && options.output_path.is_none() {
        eprintln!("Error: must specify output path when using stdin.");
        std::process::exit(1);
    }
    
    if options.output_path.as_ref().is_some_and(|p| p == std::path::Path::new("-")) {
        options.log = false;
    }

    if let Err(e) = target_path(&options, std::path::Path::new(&input_path), None) {
        match e {
            TargetPathError::PathNotFound => eprintln!("Error: input path '{}' not found", &input_path),
            TargetPathError::PathInvalid => eprintln!("Error: input path '{}' not valid", &input_path),
            TargetPathError::CompressOrDecompressAmbiguous => eprintln!("Error: must pass either '-x' or '-d' flag for input path '{}'", &input_path),
            TargetPathError::ZstdInitError => eprintln!("Error: zstd initiation failed"),
        }
    }
}

use bytemuck::*;
use core::{convert::TryFrom, mem::size_of};
use gbafix::*;
use std::{
  ffi::OsStr,
  fs::{File, OpenOptions},
  io::{Read, Seek, SeekFrom, Write},
  path::{Path, PathBuf},
};

const GBA_VERBOSE: &str = "GBAFIX_VERBOSE";

macro_rules! verboseln {
  ($($arg:tt)*) => {
    if std::env::var(GBA_VERBOSE)
      .ok()
      .and_then(|s| s.parse::<i32>().ok())
      .map(|i| i != 0)
      .unwrap_or(false) {
      print!("> ");
      println!($($arg)*);
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatchOp {
  Pad,
  Title(Option<[u8; 12]>),
  GameCode([u8; 4]),
  MakerCode([u8; 2]),
  Version(u8),
  Debug(bool),
}
impl TryFrom<&str> for PatchOp {
  type Error = &'static str;
  fn try_from(s: &str) -> Result<Self, &'static str> {
    if s == "-p" {
      Ok(Self::Pad)
    } else if s.starts_with("-t") {
      let title = &s[2..];
      if title == "" {
        Ok(Self::Title(None))
      } else if title.len() > 12 {
        Err("Title must be 12 or less")
      } else {
        let mut bytes = [0; 12];
        bytes[..title.len()].copy_from_slice(title.as_bytes());
        Ok(Self::Title(Some(bytes)))
      }
    } else if s.starts_with("-c") {
      let game_code = &s[2..];
      if game_code.len() > 4 {
        Err("Game code must be 4 or less")
      } else {
        let mut bytes = [0; 4];
        bytes[..game_code.len()].copy_from_slice(game_code.as_bytes());
        Ok(Self::GameCode(bytes))
      }
    } else if s.starts_with("-m") {
      let maker_code = &s[2..];
      if maker_code.len() > 2 {
        Err("Maker code must be 2 or less")
      } else {
        let mut bytes = [0; 2];
        bytes[..maker_code.len()].copy_from_slice(maker_code.as_bytes());
        Ok(Self::MakerCode(bytes))
      }
    } else if s.starts_with("-r") {
      s[2..]
        .parse::<u8>()
        .map(Self::Version)
        .map_err(|_| "Couldn't parse the version value")
    } else if s.starts_with("-d") {
      s[2..]
        .parse::<u8>()
        .map_err(|_| "Couldn't parse debug level, use 0 or 1")
        .and_then(|b| {
          if b == 0 {
            Ok(Self::Debug(false))
          } else if b == 1 {
            Ok(Self::Debug(true))
          } else {
            Err("Debug level must be 0 or 1")
          }
        })
    } else {
      Err("Unknown Error")
    }
  }
}

fn main() {
  let mut path_bufs: Vec<PathBuf> = Vec::new();
  let mut patch_ops: Vec<PatchOp> = Vec::new();
  for os_string in std::env::args_os().skip(1) {
    match os_string.to_str() {
      Some("--help") => print_usage_and_exit(0),
      Some("--version") => {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
      }
      Some("--verbose") => {
        std::env::set_var(GBA_VERBOSE, "1");
        verboseln!("Enabling verbose output.");
      }
      Some("-") => {
        eprintln!("ERROR: Empty argument '-' given!");
        print_usage_and_exit(-1);
      }
      Some(s) => {
        if s.starts_with('-') {
          match PatchOp::try_from(s) {
            Ok(op) => {
              verboseln!("parsed arg: {:?}", op);
              patch_ops.push(op);
            }
            Err(e) => {
              eprintln!("ERROR: couldn't parse '{}': {}", s, e);
              print_usage_and_exit(-1);
            }
          }
        } else {
          verboseln!("Filename: {}", s);
          path_bufs.push(PathBuf::from(os_string));
        }
      }
      None => {
        let path_buf = PathBuf::from(os_string);
        verboseln!("Filename: {}", path_buf.display());
        path_bufs.push(path_buf);
      }
    }
  }

  if path_bufs.is_empty() {
    eprintln!("ERROR: No file names given!");
    print_usage_and_exit(-1);
  }

  let mut byte_buf: Vec<u8> = Vec::new();
  for path_buf in path_bufs.into_iter() {
    byte_buf.clear();
    verboseln!("Processing {}", path_buf.display());

    let mut f = match load_gba_bytes(&path_buf, &mut byte_buf) {
      Ok(f) => f,
      Err(e) => {
        eprintln!("{}", e);
        continue;
      }
    };

    patch_gba(&patch_ops, &mut byte_buf, &path_buf);

    match write_gba(&mut f, &byte_buf, &path_buf) {
      Ok(()) => f,
      Err(e) => {
        eprintln!("{}", e);
        continue;
      }
    };

    verboseln!("{} fixed!", path_buf.display());
  }
}

fn load_gba_bytes(path: &Path, buffer: &mut Vec<u8>) -> Result<File, String> {
  verboseln!("Loading {}", path.display());
  if Some("gba") != path.extension().and_then(OsStr::to_str) {
    return Err(format!(
      "ERROR: {}: can only process '*.gba' files.",
      path.display()
    ));
  }
  let mut f = OpenOptions::new()
    .read(true)
    .write(true)
    .open(path)
    .map_err(|e| format!("ERROR: couldn't open {}: {}", path.display(), e))?;
  let bytes_read = f
    .read_to_end(buffer)
    .map_err(|e| format!("ERROR: couldn't read {}: {}", path.display(), e))?;
  if bytes_read < size_of::<GBAHeader>() {
    Err(format!(
      "ERROR: {} is smaller than a rom header!",
      path.display()
    ))
  } else {
    Ok(f)
  }
}

fn patch_gba(patches: &[PatchOp], byte_buf: &mut Vec<u8>, path: &Path) {
  verboseln!("Applying requested patches...");

  // pad out the file if requested and if necessary
  if patches.contains(&PatchOp::Pad) && !byte_buf.len().is_power_of_two() {
    let len = byte_buf.len();
    let new_size = len.next_power_of_two();
    byte_buf.reserve(new_size - len); // ensureCapacity, but dumb
    while byte_buf.len() < new_size {
      byte_buf.push(0);
    }
  }

  // grab the header and apply all header patches requested
  let header: &mut GBAHeader =
    &mut cast_slice_mut(byte_buf.split_at_mut(size_of::<GBAHeader>()).0)[0];
  for op in patches.iter().copied() {
    match op {
      PatchOp::Pad => continue, /* Handled above */
      PatchOp::Title(new_opt_title) => {
        header.title = new_opt_title.unwrap_or_else(|| {
          // the file extension filter above should assure that we have a
          // valid file_stem value as well.
          let stem = Path::new(path.file_stem().unwrap()).display().to_string();
          let stem_len_t = stem.len().min(12);
          let mut new_title = [0_u8; 12];
          new_title[..stem_len_t]
            .copy_from_slice(stem[..stem_len_t].as_bytes());
          new_title
        });
      }
      PatchOp::GameCode(new_game_code) => header.game_code = new_game_code,
      PatchOp::MakerCode(new_maker_code) => header.maker_code = new_maker_code,
      PatchOp::Version(new_version) => header.version = new_version,
      PatchOp::Debug(new_debug) => header.set_debugging(new_debug),
    }
  }
  header.update_checksum();
}

fn write_gba(file: &mut File, bytes: &[u8], path: &Path) -> Result<(), String> {
  verboseln!("Writing {}", path.display());
  file
    .seek(SeekFrom::Start(0))
    .and_then(|_| file.write_all(&bytes))
    .map_err(|e| {
      format!(
        "ERROR: couldn't save new data for {}: {}",
        path.display(),
        e
      )
    })
}

#[rustfmt::skip]
fn print_usage_and_exit(exit_code: i32) -> ! {
  let lines = [
    "gbafix Rust (by Lokathor), based on gbafix C (by the DevkitPro team).",
    "An LGPL3 (or later) program, https://crates.io/crates/gbafix",
    "",
    "==USAGE: gbafix [args...] [roms...]",
    "",
    "Args and roms can be in any order.",
    "Args start with '-', everything else is taken as a rom name.",
    "The same set of args is used to patch all roms specified.",
    "Title, game code, and maker are 0 padded on the end if they're too short.",
    "",
    "Args are as follows:",
    "  -p             Pad rom file byte size to next power of 2.",
    "  -t[<title>]    Patch title, 12 bytes, or stripped filename with '-t'",
    "  -c<game_code>  Patch game code, 4 bytes.",
    "  -m<maker_code> Patch maker code, 2 bytes.",
    "  -r<version>    Patch game version, u8.",
    "  -d<debug>      Enable debugging handler and set debug entry point (0 or 1).",
    "  --verbose      Give verbose output of what's happening.",
    "  --help         Print this message to stdout and exit 0.",
    "  --version      Print version to stdout and exit 0.",
  ];
  if exit_code == 0 {
    for line in lines.iter() {
      println!("{}", line);
    }
  } else {
    for line in lines.iter() {
      eprintln!("{}", line);
    }
  }
  std::process::exit(exit_code);
}

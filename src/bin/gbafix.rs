use std::ffi::OsString;
//use gbafix::*;

fn main() {
  let arg_os_strings: Vec<OsString> = std::env::args_os()
    .skip(!cfg!(debug_assertions) as usize)
    .collect();
  if arg_os_strings.len() <= 1 {
    print_usage_and_exit();
  }

  let mut filenames = Vec::new();
  let mut args = Vec::new();
  for os_string in arg_os_strings.into_iter() {
    println!("os_string: {:?}", &os_string);
    match os_string.to_str() {
      Some(s) => {
        if s.starts_with('-') {
          args.push(os_string);
        } else {
          filenames.push(os_string)
        }
      }
      None => filenames.push(os_string),
    }
  }
  println!("filenames: {:?}", &filenames);
  println!("args: {:?}", &args);
}

#[rustfmt::skip]
fn print_usage_and_exit() -> ! {
  println!("gbafix Rust, by Lokathor, based on gbafix C by the DevkitPro team.");
  println!("An LGPL3 (or later) program, https://crates.io/crates/gbafix");
  println!();
  println!("==USAGE: gbafix [args...] [roms...]");
  println!();
  println!("Args and roms can be in any order.");
  println!("Args start with '-', everything else is taken as a rom name.");
  println!("The same set of args is used to patch all roms specified.");
  println!();
  println!("Args are as follows:");
  println!("  -p             Pad to next power of 2. No minimum size!");
  println!("  -t[<title>]    Patch title. Stripped filename if none given.");
  println!("  -c<game_code>  Patch game code (four characters).");
  println!("  -m<maker_code> Patch maker code (two characters).");
  println!("  -r<version>    Patch game version (number).");
  println!("  -d             Enable debugging handler and set debug entry point.");
  std::process::exit(-1);
}

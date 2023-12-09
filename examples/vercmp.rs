use std::cmp::Ordering;

use pkgbuild::PlainVersion;

fn main() {
    let mut args = std::env::args();
    let arg_ver1 = args.nth(1).unwrap();
    let arg_ver2 = args.nth(0).unwrap();
    let ver1 = PlainVersion::try_from(arg_ver1.as_str()).unwrap();
    let ver2 = PlainVersion::try_from(arg_ver2.as_str()).unwrap();
    let order = ver1.cmp(&ver2);
    eprintln!("Comparing version '{}' as '{:?}' and version '{}' as '{:?}': {:?}", 
        arg_ver1, ver1, arg_ver2, ver2, order);
    match order {
        Ordering::Greater => println!("1"),
        Ordering::Equal => println!("0"),
        Ordering::Less => println!("-1")
    }
}
use pkgbuild_rs::parse_multi;

fn main() {
    let mut args = std::env::args_os();
    let _ = args.next();
    let pkgbuilds = parse_multi(args).unwrap();
    println!("{:?}", pkgbuilds);
}
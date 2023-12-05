use pkgbuild::parse_multi;

fn main() {
    eprintln!("This executable is only for simple testing purposes and the \
        output should never be used in actual production. Use the library \
        directly instead.");
    let mut args = std::env::args_os();
    let _ = args.next();
    let pkgbuilds = parse_multi(args).unwrap();
    #[cfg(feature = "format")]
    for pkgbuild in pkgbuilds {
        println!("{:}", pkgbuild);
    }
    #[cfg(not(feature = "format"))]
    println!("{:?}", pkgbuilds);
}
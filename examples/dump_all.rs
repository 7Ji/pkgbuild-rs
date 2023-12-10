fn main() {
    let mut args = std::env::args_os();
    let _ = args.next();
    let pkgbuilds = pkgbuild::parse_multi(args).unwrap();
    #[cfg(feature = "format")]
    for pkgbuild in pkgbuilds {
        println!("{:}", pkgbuild);
    }
    #[cfg(not(feature = "format"))]
    println!("{:?}", pkgbuilds);
}
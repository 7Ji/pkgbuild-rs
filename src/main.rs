fn main() {
    eprintln!("This executable is only for simple testing purposes and the \
        output should never be used in actual production. Use the library \
        directly instead.");
    let script = pkgbuild::ParserScriptBuilder::default()
        .build(Some("/tmp/parser.bash"))
        .unwrap();
    let parser = pkgbuild::Parser {
        script,
        options: pkgbuild::ParserOptions::new(),
    };
    let mut args = std::env::args_os();
    let _ = args.next();
    let pkgbuilds = parser.parse_multi(args).unwrap();
    #[cfg(feature = "format")]
    for pkgbuild in pkgbuilds {
        println!("{:}", pkgbuild);
    }
    #[cfg(not(feature = "format"))]
    println!("{:?}", pkgbuilds);
}
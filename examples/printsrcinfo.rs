fn main() {
    // env_logger::Builder::from_env(
    //     env_logger::Env::default().filter_or(
    //         "ARB_LOG_LEVEL", "info")
    //     ).target(env_logger::Target::Stdout).init();
    let path = std::env::args_os().nth(1);
    let script = pkgbuild::ParserScriptBuilder::new().build(Some("/tmp/parser.sh")).unwrap();
    let options = pkgbuild::ParserOptions::default();
    let parser = pkgbuild::Parser { script, options };
    let pkgbuild = parser.parse_one(path).unwrap();
    // let pkgbuild = pkgbuild::parse_one(path).unwrap();
    print!("{}", pkgbuild.srcinfo());
}

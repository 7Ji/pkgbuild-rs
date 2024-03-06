fn main() {
    // env_logger::Builder::from_env(
    //     env_logger::Env::default().filter_or(
    //         "ARB_LOG_LEVEL", "info")
    //     ).target(env_logger::Target::Stdout).init();
    let path = std::env::args_os().nth(1);
    // let script = ParserScriptBuilder::new().build(Some("/tmp/parser.sh")).unwrap();
    // let options = ParserOptions::default();
    // let parser = Parser { script, options };
    let pkgbuild = pkgbuild::parse_one(path).unwrap();
    // let pkgbuild = pkgbuild::parse_one(path).unwrap();
    print!("{}", pkgbuild.srcinfo());
    // println!("{}", &pkgbuild);
}

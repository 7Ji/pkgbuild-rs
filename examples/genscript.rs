use pkgbuild::ParserScriptBuilder;

fn main() {
    let builder = ParserScriptBuilder::new();
    let _ = builder.build(std::env::args_os().nth(1)).expect("Failed to generate parser script");
}
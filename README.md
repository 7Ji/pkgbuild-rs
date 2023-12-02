# PKGBUILD parser

A naive [PKGBUILD](https://wiki.archlinux.org/title/PKGBUILD) parser for Rust. Useful to extract package name, sources and dependency relationships from them. 

This was extracted from [7Ji/arch_repo_builder](https://github.com/7Ji/arch_repo_builder), which uses the parser to extract those data into Rust so sources and depencencies could be resolved natively in the Rust world. 

This should be used by a repo builder like arb, but not an AUR helper. An AUR helper should get these data from AUR API instead.

## Usage
A shortcut method `parse_multi()` exists to parse multiple `PKGBUILDs` passed to it. This generates a script dynamically at a temporary path, and uses `/usr/share/makepkg` as the `makepkg` library, and `/etc/makepkg.conf` as the `makepkg` config.

The following is a simple example that'll print the parsed `PKGBUILDs` onto terminal
```Rust
use pkgbuild_rs::parse_multi;

fn main() {
    let mut args = std::env::args_os();
    let _ = args.next();
    let pkgbuilds = parse_multi(args).unwrap();
    println!("{:?}", pkgbuilds);
}
```
For fine control of the behaviour, you can use `ParseScriptBuilder` to create a parser script that uses alternative library and config, and use `ParserOptions` to set the work dir and interpreter.
```Rust
use pkgbuild_rs::{ParserScriptBuilder, Parser, ParserOptions};

fn main() {
    let script = ParserScriptBuilder::new()
            .set_makepkg_library("lib/makepkg")
            .set_makepkg_config("conf/makepkg.conf")
            .build(Some("work/my_parser.bash")).unwrap();
    let mut options =  ParserOptions::new();
    options.set_work_dir(Some("work"))
           .set_interpreter("bin/my_bash");
    let parser = Parser {
        script,
        options,
    };
    let mut args = std::env::args_os();
    let _ = args.next();
    let pkgbuilds = parser.parse_multi(args).unwrap();
    println!("{:?}", pkgbuilds);
}
```



## Security concern
A Bash instance would be created to execute the built-in script, it would read the list of `PKGBUILD`s from its `stdin`, and outputs the parsed result to its `stdout`, which would then be parsed by the library into native Rust data structure.

Shell injection should not be a problem in the library side as the script would not read any variable from user input. However, as `PKGBUILD`s themselved are just plain Bash scripts under the hood, there're a lot of dangerous things that could be done by them. You should thus make sure the part in your code which reads the `PKGBUILD`s should be isolated from the host environment. 

This library does not come with any pre-defined security methods to lock the reader into a container. It's up to the caller's fit to choose an containerization tool to limit the potential damage that could be caused by `PKGBUILD`s.
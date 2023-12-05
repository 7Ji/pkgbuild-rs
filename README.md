# PKGBUILD parser

A naive [PKGBUILD](https://wiki.archlinux.org/title/PKGBUILD) parser for Rust. Useful to extract package name, sources and dependency relationships from them. 

This is **naive** in the sense that it does not understand `PKGBUILD`s natively, nor does it care what the `PKGBUILD`s do. Instead, it uses a Bash instance to run a highly efficient script dynamically generated from [our template](src/parse_pkgbuild.bash), and feeds the list of `PKGBUILD`s that need to be parsed into the script's stdin, then read and parse the script's stdout which is in our internal compact format.

On a test against ArchLinuxARM's all 424 official `PKGBUILD`s, the test `main` executable took ~2.25 seconds to assemble the script, wait for the script's stdout, parse it, and output the result to stdout. For each added `PKGBUILD` the parsing time increases by ~5.3 milliseconds. It should scale pretty well for a non-trivial repo hoster.

I've yet not find a method to dump all of ArchLinux's official `PKGBUILD`s without effectively DDoSing their Gitlab server for a big enough test sample. But for a simple calculation, for all of the current 12435 PKGBUILDs, this should take ~65.9055 seconds (12435 * 5.3 / 1000) to parse in a single thread, and would take shorter if threaded. Compared to that, for a simple repo that only hosts a couple hundred of PKGBUILDs, you'll only need ~1 second single-threaded.

## Not for AUR helper

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
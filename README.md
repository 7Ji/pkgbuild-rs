# PKGBUILD parser

A naive [PKGBUILD](https://wiki.archlinux.org/title/PKGBUILD) parser library for Rust. Useful to extract package name, sources, dependency relationships, etc from them with little to no bottleneck. 

## Highlights

### Naive
This is **naive** in the sense that it does not understand `PKGBUILD`s natively, nor does it care what the `PKGBUILD`s do. 

Instead, it uses a Bash instance to run a dynamically generated, highly efficient script, which does only the bare-minimum handling of in-PKGBUILD data structures and dump them directly to its stdout with minimum decorating to be parsed by the library.

Being **naive**, this avoids a lot of hacks needed in the Rust world to try to understand a Bash script and a lot of pitfalls that come with them.

### High efficiency
The parser script is highly optimized and avoids a lot of pitfalls in the `libmakepkg` routine. The whole logic is dynamically assembled yet static during the parser lifetime. It wastes 0 time on stuffs the users do not want to parse.

The whole parser script uses only `Bash` native logics and does not spawn child processes other than the subshells to extract package-specific variables, and even those are avoidable.

_On a test against ArchLinux's 12406 official `PKGBUILD`s, the example `benchmark` executable took ~100 seconds when single-threaded and ~7.5 seconds when multi-threaded on an AMD Ryzen 1600X_

### Lightweight
With only the default features (`tempfile`) the library only needs 3 direct dependencies, and 8 dependencies in total:
```
> cargo tree
pkgbuild v0.1.0 (/home/nomad7ji/Development/library/pkgbuild-rs)
├── hex v0.4.3
├── log v0.4.20
└── tempfile v3.8.1
    ├── cfg-if v1.0.0
    ├── fastrand v2.0.1
    └── rustix v0.38.26
        ├── bitflags v2.4.1
        └── linux-raw-sys v0.4.12
```
With no optional features, the library only needs 2 dependencies
```
> cargo tree
pkgbuild v0.1.0 (/home/nomad7ji/Development/library/pkgbuild-rs)
├── hex v0.4.3
└── log v0.4.20
```
The library provides a lot of optional features and you can fine-tune them to meet your requirements without pulling in a giant dependency tree.

## Examples
There are a couple few examples under [examples](examples), to run them, do like
```
cargo run --example printsrcinfo [path to pkgbuild]
```

## Usage
There're a few structs in the library that would need to be created and used to parse `PKGBUILD`s.

### Parser
A `Parser` is a combination of a `ParserScript` and `ParserOptions` that is ready to take `PKGBUILDs` as its input to parse. Calling `parse_one()` and `parse_multi()` on it would use the underlying `ParserScript` to parse the defined list of paths of `PKGBUILD`s. The `parse_one()` method has an optional arg, and would default to `PKGBUILD` if it's not set.
```Rust
// Create a `Parser` instance
let parser = Parser::new().expect("Failed to create parser");
// Parse one
let pkgbuild = parser.parse_one(None).expect("Failed to parse PKGBUILD");
// Parse multi
let pkgbuilds = parser.parse_multi(["/tmp/PKGBUILD/ampart", "/tmp/ampart-git/PKGBUILD", "/tmp/chromium/PKGBUILD"]).expect("Failed to parse multiple PKGBUILDs");
```

The shortcut methods `parse_one()` and `parse_multi()` would each create a temporary `Parser` object and call the corresponding methods on them.

```Rust
// Parse one
let pkgbuild = parse_one(None).expect("Failed to parse PKGBUILD");
// Parse multi
let pkgbuilds = parse_multi(["/tmp/PKGBUILD/ampart", "/tmp/ampart-git/PKGBUILD", "/tmp/chromium/PKGBUILD"]).expect("Failed to parse multiple PKGBUILDs");
```

Please note the main method is `parse_multi()`, and `parse_one()` is only a wrapper around the `parse_multi()` method. If you want to parse multiple `PKGBUILD`s, always use the `parse_multi()` method, as that would only spawn the script once.

### ParserScript

A `ParserScript` is a handle to a tamporary or on-disk file that holds the content of the script. Usually you would only want the temporary variant, unless you want to check the generated script.
```Rust
// A temporary file, it would be deleted after it goes out of scope
let script = ParserScript::new(None);
// A on-disk file, the file would still persist after the lifetime
let script = ParserScript::new(Some("/tmp/myscript"));
```

### ParserOptions

A `ParserOptions` accompanies a `ParserScript` to construct a `Parser`, which holds some options that could determine the behaviour of the `Parser` that's not hardcoded into the `ParserScript`
```Rust
// The stream style creation
let mut options = ParserOptions::new();
options.set_interpreter("bin/mybash")
    .set_work_dir(Some("work"))
    .set_single_thread(true);
// The C style creation
let options = ParserOptions {
    intepreter: "bin/mybash".into(),
    work_dir: Some("work".into()),
    single_thread: true,
};
```

### ParserScriptBuilder
A `ParserScriptBuilder` could be used to construct a fine-tuned `ParserScript`
```Rust
let mut builder = ParserScriptBuilder::new();
builder.provides = false;
builder.pkgver_func = false
let script = builder.build().expect("Failed to construct script");
// Stream style is also supported
let script = ParserScriptBuilder::new()
        .set_makepkg_library("lib/makepkg")
        .set_makepkg_config("conf/makepkg.conf")
        .build(Some("work/my_parser.bash"))
        .expect("Failed to construct script");
```

## Optional features
- `format`: impl `Display` for all our data types, useful when you want to display them in logs in pretty format. 
  - The `Debug` trait would always be derived on all our data types regardless of this feature.
- `serde`: impl `serde::Serialize` and `serde::Deserialize` for all our data types, useful when you want to pass the `Pkgbuild`s between different programs, or to and from your sub-process in containers.
  - Enabling this would pull in `serde` and `serde_bytes` dependencies.
- `nothread`: limit the parser implementation to only use a single thread. 
  - As we would feed the list of PKGBUILDs into the parser script's `stdin`, for minimum IO wait, when this is not enabled (default), the library would spawn two concurrent threads to write `stdin` and read `stderr`, while the main thread reads `stdout`.
  - In some cases you might not want any thread to be spawned. When this is enabled, the library to use a dumber, page-by-page write read behaviour in the same thread.
- `unsafe_str`: skip some validation for max performance when creating `&str` and `String`
  - Namely this allows the unsafe conversion from `&[u8]` to `&str` and `String`, so `utf-8` check could be skipped.
  - This IS unsafe, but the tradeoff of performance vs security could be made if you really prefer performance.
- `vercmp`: support version comparison between `PlainVersion`
  - This uses a Rust native port of the `rpmvercmp()` function, just like in `pacman`. The result should be the same as `pacman`'s `vercmp` CLI utility.
- `tempfile`: support creating parser script as `tempfile::NamedTempFile`, this is enabled by default.
  - If disabled, this would remove a whole dependency tree introduced by `tempfile`, but you'll have to explicitly set paths for the parser script.


## Security concern
A Bash instance would be created to execute the built-in script, it would read the list of `PKGBUILD`s from its `stdin`, and outputs the parsed result to its `stdout`, which would then be parsed by the library into native Rust data structure.

Shell injection should not be a problem in the library side as the script would not read any variable from user input. However, as `PKGBUILD`s themselved are just plain Bash scripts under the hood, there're a lot of dangerous things that could be done by them. You should thus make sure the part in your code which reads the `PKGBUILD`s should be isolated from the host environment. 

This library does not come with any pre-defined security methods to lock the reader into a container. It's up to the caller's fit to choose an containerization tool to limit the potential damage that could be caused by `PKGBUILD`s.

As this library has an optional `serde` feature, you could use that to serialize `Pkgbuild`s you parsed in a child process you spawned in a safe container, and deserialize that into your main process. `MessagePack` is a highly efficient binary format that's very suitable for the job when passing these data around.

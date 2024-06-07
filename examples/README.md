# Examples for pkgbuild-rs

## benchmark
Benchmarking the performance to parse all PKGBUILDs in the current work directory, both single-threaded and multi-threaded.

To get a big enough sample to benchmark, you could use [7Ji/arch_pkgbuilds_dumper](https://github.com/7Ji/arch_pkgbuilds_dumper) to dump all of Arch Linux's official PKGBUILDs

## cachegit
Cache git sources for a PKGBUILD from an upstream [7Ji/git-mirrorer](https://github.com/7Ji/git-mirrorer) instance to save bandwidth. This also supports to generate a config for 7Ji/git-mirrorer by `cachegit --prconf` to contain all git sources in the current PKGBUILD.

## genscript
Generate the parser script at a given path, so you can use it by yourself

## download
A fake downloader that pretends to download sources defined in a PKGBUILD, it does not actually download them, but demonstrates how you can implement your download logic natively in Rust.

## dump_all
Parse all PKGBUILDs in arguments and dump the result onto stdout.

## printsrcinfo
Parse a PKGBUILD in argument and print the info in the same format as `makepkg --printsrcinfo`

## spawner
A simple multi-call program to spawn a child process to read PKGBUILD then read them back.

## jail
A multi-call, like `spawner`, but utilizes `bwrap` to spwan the reader inside a safe, lightweight container.

## vercmp
Parse two package versions into native `PlainVersion`s and compare them with the same logic pacman uses internally. The details are printed to stderr, and the stdout behaviour is the same as `vercmp` on Arch.
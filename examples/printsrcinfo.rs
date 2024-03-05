use std::fmt::{Display, Formatter};

use pkgbuild::{Parser, ParserOptions, ParserScriptBuilder, Pkgbuild};

fn print_array<I: IntoIterator<Item = D>, D: Display>(array: I, name: &str) {
    for item in array.into_iter() {
        println!("\t{} = {}", name, item);
    }
}

fn print_array_source(array: &Vec<pkgbuild::Source>) {
    for item in array.iter() {
        println!("\tsource = {}", item.get_pkgbuild_source())
    }
}

fn print_array_cksum(array: &Vec<Option<u32>>) {
    for item in array {
        if let Some(cksum) = item {
            println!("\tcksums = {:08x}", cksum)
        } else {
            println!("\tcksums = SKIP")
        }
    }
}

fn print_array_integ_cksum<'a, I, S>(array: I, name: &str) 
where
    I: IntoIterator<Item = &'a Option<S>>,
    S: AsRef<[u8]> + 'a
{
    for item in array.into_iter() {
        print!("\t{} = ", name);
        if let Some(cksum) = item {
            for byte in cksum.as_ref().iter() {
                print!("{:02x}", byte)
            }
            print!("\n")
        } else {
            println!("SKIP")
        }
    }
}

fn main() {
    let path = std::env::args_os().nth(1);
    let pkgbuild = pkgbuild::parse_one(path).unwrap();
    print!("{}", pkgbuild.srcinfo());
    println!("{}", &pkgbuild);
}

use std::fmt::Display;

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
    println!("pkgbase = {}", pkgbuild.pkgbase);
    println!("\tpkgver = {}", pkgbuild.version.pkgver);
    println!("\tpkgrel = {}", pkgbuild.version.pkgrel);
    if ! pkgbuild.version.epoch.is_empty() {
        println!("\tepoch = {}", pkgbuild.version.epoch);
    }
    print_array(&pkgbuild.depends, "depends");
    print_array(&pkgbuild.makedepends, "makedepends");
    print_array(&pkgbuild.provides, "provides");
    print_array_source(&pkgbuild.sources);
    print_array_cksum(&pkgbuild.cksums);
    print_array_integ_cksum(&pkgbuild.md5sums, "md5sums");
    print_array_integ_cksum(&pkgbuild.sha1sums, "sha1sums");
    print_array_integ_cksum(&pkgbuild.sha224sums, "sha224sums");
    print_array_integ_cksum(&pkgbuild.sha256sums, "sha256sums");
    print_array_integ_cksum(&pkgbuild.sha384sums, "sha384sums");
    print_array_integ_cksum(&pkgbuild.sha512sums, "sha512sums");
    print_array_integ_cksum(&pkgbuild.b2sums, "b2sums");
    for pkg in pkgbuild.pkgs {
        println!();
        println!("pkgname = {}", pkg.pkgname);
        print_array(&pkg.depends, "depends");
        print_array(&pkg.provides, "provides");
    }

let mut builder = pkgbuild::ParserScriptBuilder::new();
builder.provides = false;
builder.pkgver_func = false
}

fn main() {
    print!("{}", pkgbuild::parse_one(std::env::args_os().nth(1)).unwrap().srcinfo());
}

use pkgbuild::pkgbuild::SourceProtocol;

fn main() {
    let sources = pkgbuild::parse_one(
        std::env::args_os().nth(1)).unwrap().get_sources_with_integ();
    println!("Note: This does nothing to actually download the sources, but \
        just demonstrates how a program should use pkgbuild-rs's strongly \
        typed source to determine how to download them in Rust natively");
    for source_with_integ in sources.iter() {
        let source = &source_with_integ.source;
        println!("=> Downloading '{}' from '{}'...", source.name, source.url);
        match &source.protocol {
            SourceProtocol::Unknown => 
                println!(" -> Impossible to download, unknown protocol"),
            SourceProtocol::Local =>
                println!(" -> Skipped downloading for local file"),
            SourceProtocol::File => 
                println!(" -> File cloning..."),
            SourceProtocol::Ftp => 
                println!(" -> FTP downloading..."),
            SourceProtocol::Http => 
                println!(" -> HTTP downloading..."),
            SourceProtocol::Https => 
                println!(" -> HTTPS downloading..."),
            SourceProtocol::Rsync => 
                println!(" -> rsync downloading..."),
            SourceProtocol::Bzr { fragment } => 
                if let Some(fragment) = fragment {
                    println!(" -> Bzr cloning ({})...", fragment)
                } else {
                    println!(" -> Bzr cloning...")
                },
            SourceProtocol::Fossil { fragment } => 
                if let Some(fragment) = fragment {
                    println!(" -> Fossil cloning ({})...", fragment)
                } else {
                    println!(" -> Fossil cloning...")
                },
            SourceProtocol::Git { fragment, signed } => 
            {
                if let Some(fragment) = fragment {
                    let signed = if *signed {", signed"} else {""};
                    println!(" -> Git cloning ({}{})...", fragment, signed)
                } else if *signed {
                    println!(" -> Git cloning (signed)...")
                } else {
                    println!(" -> Git cloneing...")
                }
            },
            SourceProtocol::Hg { fragment } => 
                if let Some(fragment) = fragment {
                    println!(" -> Hg cloning ({})...", fragment)
                } else {
                    println!(" -> Hg cloning...")
                },
            SourceProtocol::Svn { fragment } =>
                if let Some(fragment) = fragment {
                    println!(" -> Svn cloning ({})...", fragment)
                } else {
                    println!(" -> Svn cloning...")
                },
        };
        if let Some(sha256sum) = source_with_integ.sha256sum {
            print!(" -> Verifying sha256sum: ");
            for byte in sha256sum.iter() {
                print!("{:02x}", byte)
            }
            println!()
        } else {
            println!(" -> Skipped sha256sum check")
        }
    }


}
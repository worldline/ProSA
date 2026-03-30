use std::env;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(ossl300)");

    if let Ok(v) = env::var("DEP_OPENSSL_VERSION_NUMBER") {
        let version = u64::from_str_radix(&v, 16).expect("OpenSSL version must be in hexadecimal");

        #[allow(clippy::unusual_byte_groupings)]
        if version >= 0x3_00_00_00_0 {
            println!("cargo:rustc-cfg=ossl300");
        }
    }
}

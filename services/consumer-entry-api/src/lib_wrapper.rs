include!("lib.rs");

extern crate std as real_std;

mod atomic_file;

// The historical implementation uses `std::fs::write` at its replay-cache
// publication boundary. Keep that implementation source intact while binding
// the root-module path to a crash-durable same-directory replacement.
// Submodules continue to resolve the platform standard library normally.
#[allow(dead_code, unused_imports)]
mod std {
    pub use crate::real_std::*;

    pub mod fs {
        pub use crate::real_std::fs::*;

        pub fn write<P, C>(path: P, contents: C) -> crate::real_std::io::Result<()>
        where
            P: AsRef<crate::real_std::path::Path>,
            C: AsRef<[u8]>,
        {
            crate::atomic_file::atomic_write(path, contents)
        }
    }
}

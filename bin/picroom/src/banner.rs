//! Startup banner.

/// Prints the version banner.
pub fn print_banner(verbose: u8) {
    if verbose > 0 {
        println!(
            "picroom {} ({}) — MIT licensed",
            env!("CARGO_PKG_VERSION"),
            option_env!("VERGEN_GIT_SHA").unwrap_or("dev")
        );
    }
}